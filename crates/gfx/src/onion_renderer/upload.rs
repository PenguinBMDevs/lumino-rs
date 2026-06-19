use super::{DrawIndirectArgs, OnionNote, OnionRenderer, OnionViewportUniform};

/// 洋葱皮音符上传参数包（避免 `upload_notes` 参数超过 clippy 阈值）
pub struct OnionUploadParams<'a> {
    /// 待上传的洋葱皮音符列表
    pub notes: &'a [OnionNote],
    /// 音符列表版本号（dirty tracking）
    pub list_version: u64,
    /// 每音轨打包颜色表（index = track_idx）
    pub track_colors: &'a [u32],
    /// 颜色表哈希（dirty tracking）
    pub color_hash: u64,
    /// 当前视口（用于 CPU 端过滤，只上传可见音符）
    pub viewport: &'a OnionViewportUniform,
    /// wgpu 设备
    pub device: &'a wgpu::Device,
    /// wgpu 队列
    pub queue: &'a wgpu::Queue,
}

impl OnionRenderer {
    /// 上传洋葱皮音符到 GPU（颜色注入 + 视口过滤 + storage buffer）
    ///
    /// # 数据流
    /// 1. 按视口过滤：只保留当前可见（含 overscan）区域内的音符，避免 160M
    ///    音符全量上传撑爆 GPU storage buffer
    /// 2. 颜色注入（per-note color_packed）
    /// 3. write_buffer 到 GPU note_pool_buffer
    /// 4. Compute shader 随后通过 prepare_cull 读取 note_pool_buffer 做剔除
    pub fn upload_notes(&mut self, params: OnionUploadParams<'_>) {
        let vp_hash = Self::viewport_hash(params.viewport);

        // Dirty tracking：note list / 颜色 / 视口 都未变化 → 跳过
        if params.list_version == self.last_list_version
            && params.color_hash == self.last_color_hash
            && vp_hash == self.last_upload_viewport_hash
        {
            return;
        }

        // 按视口过滤 + 颜色注入
        let max_capacity = (self.max_storage_binding as usize / std::mem::size_of::<OnionNote>())
            .min(Self::MAX_NOTE_POOL_CAPACITY);

        self.cpu_note_pool.clear();
        self.cpu_note_pool.reserve(params.notes.len().min(max_capacity));

        let mut dropped = 0usize;
        for note in params.notes {
            if self.cpu_note_pool.len() >= max_capacity {
                dropped += 1;
                continue;
            }

            // 视口剔除：不在当前可见区域内的音符不上传
            let pitch = note.pitch() as f32;
            if pitch < params.viewport.pitch_min || pitch > params.viewport.pitch_max {
                continue;
            }
            if (note.end_tick as f32) <= params.viewport.tick_start
                || (note.start_tick as f32) >= params.viewport.tick_end
            {
                continue;
            }

            let color = params
                .track_colors
                .get(note.track_idx() as usize)
                .copied()
                .unwrap_or(0);
            let mut colored = *note;
            colored.set_color_packed(color);
            self.cpu_note_pool.push(colored);
        }

        if dropped > 0 {
            tracing::warn!(
                "Onion note pool capacity exceeded: dropped {} notes (capacity {})",
                dropped,
                max_capacity
            );
        }

        let uploaded = self.cpu_note_pool.len();
        self.total_note_count = uploaded as u32;

        // 按需扩容 note_pool
        let required = uploaded.max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity && self.note_pool_capacity < max_capacity {
            let new_cap = if self.note_pool_capacity == Self::INITIAL_NOTE_CAPACITY {
                max_capacity
            } else {
                required.next_power_of_two().min(max_capacity)
            };
            if new_cap > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(params.device, new_cap);
                self.note_pool_capacity = new_cap;
                self.rebuild_bind_groups(params.device);
            }
        }

        // 按需扩容 instance_indices
        let required_idx = uploaded.max(Self::INITIAL_INDICES_CAPACITY);
        if required_idx > self.indices_capacity {
            let new_cap = required_idx
                .next_power_of_two()
                .min(Self::MAX_INDICES_CAPACITY);
            if new_cap > self.indices_capacity {
                self.instance_indices_buffer =
                    Self::create_instance_indices_buffer(params.device, new_cap);
                self.indices_capacity = new_cap;
                self.rebuild_bind_groups(params.device);
            }
        }

        let upload_count = uploaded.min(self.note_pool_capacity);
        params.queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..upload_count]),
        );

        self.last_list_version = params.list_version;
        self.last_color_hash = params.color_hash;
        self.last_upload_viewport_hash = vp_hash;
    }

    /// 计算视口几何哈希（仅用于 CPU 过滤 dirty tracking）
    fn viewport_hash(vp: &OnionViewportUniform) -> u64 {
        let mut h = 0u64;
        h = h.wrapping_mul(31).wrapping_add(vp.tick_start.to_bits() as u64);
        h = h.wrapping_mul(31).wrapping_add(vp.tick_end.to_bits() as u64);
        h = h.wrapping_mul(31).wrapping_add(vp.pitch_min.to_bits() as u64);
        h = h.wrapping_mul(31).wrapping_add(vp.pitch_max.to_bits() as u64);
        h
    }

    /// GPU 端视口剔除 — dispatch compute shader
    ///
    /// 每个线程处理一个音符，通过 atomicAdd 写入 instance_indices buffer。
    /// compute shader 同时清零 indirect_args（instance_count=0），
    /// 无需 CPU write_buffer。
    pub fn prepare_cull(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &OnionViewportUniform,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if self.total_note_count == 0 {
            return;
        }

        // Dirty tracking：视口未变 → 跳过 compute dispatch
        let vp_changed = self.last_viewport.as_ref() != Some(viewport);
        if !vp_changed {
            return;
        }

        // CPU 端重置 indirect_args（compute shader 需要从 instance_count=0 开始 atomicAdd）
        queue.write_buffer(
            &self.indirect_buffer,
            0,
            bytemuck::cast_slice(&[DrawIndirectArgs {
                vertex_count: 4,
                instance_count: 0,
                first_vertex: 0,
                first_instance: 0,
            }]),
        );

        // 更新 viewport uniform（compute shader 读取 viewport 做剔除）
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[*viewport]));

        // Dispatch compute cull（WGSL 限制每个维度 ≤ 65535，用 2D dispatch 规避）
        const MAX_DISPATCH_X: u32 = 65535;
        let workgroup_count = self.total_note_count.div_ceil(Self::WORKGROUP_SIZE).max(1);
        let dispatch_x = workgroup_count.min(MAX_DISPATCH_X);
        let dispatch_y = workgroup_count.div_ceil(MAX_DISPATCH_X);
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("onion_cull_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
        compute_pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);

        self.last_viewport = Some(*viewport);
    }

    /// 准备视口 uniform（已由 prepare_cull 写入，保留接口兼容性）
    #[allow(unused_variables)]
    pub fn prepare_viewport(&mut self, viewport: &OnionViewportUniform, queue: &wgpu::Queue) {}

    /// 间接绘制裁剪后的洋葱皮音符
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if self.total_note_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.draw_indirect(&self.indirect_buffer, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::OnionRenderer;
    use crate::OnionViewportUniform;

    #[test]
    fn viewport_hash_same_viewport_equal() {
        let vp = OnionViewportUniform {
            tick_start: 100.0,
            tick_end: 500.0,
            pitch_min: 20.0,
            pitch_max: 80.0,
            ..OnionViewportUniform::default()
        };
        assert_eq!(
            OnionRenderer::viewport_hash(&vp),
            OnionRenderer::viewport_hash(&vp)
        );
    }

    #[test]
    fn viewport_hash_different_viewport_differs() {
        let a = OnionViewportUniform {
            tick_start: 100.0,
            tick_end: 500.0,
            pitch_min: 20.0,
            pitch_max: 80.0,
            ..OnionViewportUniform::default()
        };
        let b = OnionViewportUniform {
            tick_start: 100.0,
            tick_end: 501.0,
            pitch_min: 20.0,
            pitch_max: 80.0,
            ..OnionViewportUniform::default()
        };
        assert_ne!(OnionRenderer::viewport_hash(&a), OnionRenderer::viewport_hash(&b));
    }

    #[test]
    fn viewport_hash_ignores_note_count() {
        let a = OnionViewportUniform {
            tick_start: 100.0,
            tick_end: 500.0,
            pitch_min: 20.0,
            pitch_max: 80.0,
            note_count: 0,
            ..OnionViewportUniform::default()
        };
        let b = OnionViewportUniform {
            tick_start: 100.0,
            tick_end: 500.0,
            pitch_min: 20.0,
            pitch_max: 80.0,
            note_count: 9999,
            ..OnionViewportUniform::default()
        };
        assert_eq!(OnionRenderer::viewport_hash(&a), OnionRenderer::viewport_hash(&b));
    }
}
