use super::{DrawIndirectArgs, OnionNote, OnionRenderer, OnionViewportUniform};

impl OnionRenderer {
    /// 上传全量洋葱皮音符到 GPU（颜色注入 + storage buffer）
    ///
    /// # 数据流
    /// 1. 颜色注入（per-note color_packed）
    /// 2. write_buffer 到 GPU note_pool_buffer
    /// 3. Compute shader 随后通过 prepare_cull 读取 note_pool_buffer 做剔除
    pub fn upload_notes(
        &mut self,
        notes: &[OnionNote],
        list_version: u64,
        track_colors: &[u32],
        color_hash: u64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        // Dirty tracking
        if list_version == self.last_list_version && color_hash == self.last_color_hash {
            return;
        }

        let count = notes.len();
        self.total_note_count = count as u32;

        if count == 0 {
            self.cpu_note_pool.clear();
            self.last_list_version = list_version;
            self.last_color_hash = color_hash;
            return;
        }

        // 颜色注入 + CPU 缓存
        self.cpu_note_pool.clear();
        self.cpu_note_pool.reserve(count);
        if track_colors.is_empty() {
            self.cpu_note_pool.extend_from_slice(notes);
        } else {
            for note in notes {
                let color = track_colors
                    .get(note.track_idx() as usize)
                    .copied()
                    .unwrap_or(0);
                let mut colored = *note;
                colored.set_color_packed(color);
                self.cpu_note_pool.push(colored);
            }
        }

        // 按需扩容 note_pool
        let max_capacity = (self.max_storage_binding as usize / std::mem::size_of::<OnionNote>())
            .min(Self::MAX_NOTE_POOL_CAPACITY);
        let required = count.max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity && self.note_pool_capacity < max_capacity {
            let new_cap = if self.note_pool_capacity == Self::INITIAL_NOTE_CAPACITY {
                max_capacity
            } else {
                required.next_power_of_two().min(max_capacity)
            };
            if new_cap > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(device, new_cap);
                self.note_pool_capacity = new_cap;
                self.rebuild_bind_groups(device);
            }
        }

        // 按需扩容 instance_indices
        let required_idx = count.max(Self::INITIAL_INDICES_CAPACITY);
        if required_idx > self.indices_capacity {
            let new_cap = required_idx
                .next_power_of_two()
                .min(Self::MAX_INDICES_CAPACITY);
            if new_cap > self.indices_capacity {
                self.instance_indices_buffer =
                    Self::create_instance_indices_buffer(device, new_cap);
                self.indices_capacity = new_cap;
                self.rebuild_bind_groups(device);
            }
        }

        let upload_count = count.min(self.note_pool_capacity);
        queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..upload_count]),
        );

        self.last_list_version = list_version;
        self.last_color_hash = color_hash;
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
