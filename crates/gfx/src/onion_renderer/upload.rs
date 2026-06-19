use super::{DrawIndirectArgs, OnionRenderer, OnionViewportUniform};
use crate::OnionNoteList;

/// 洋葱皮音符上传参数包（避免 `upload_notes` 参数超过 clippy 阈值）
pub struct OnionUploadParams<'a> {
    /// 洋葱皮音符列表（按时间分块）
    pub note_list: Option<&'a OnionNoteList>,
    /// 音符列表版本号（dirty tracking）
    pub list_version: u64,
    /// 每音轨打包颜色表（index = track_idx）
    pub track_colors: &'a [u32],
    /// 颜色表哈希（dirty tracking）
    pub color_hash: u64,
    /// 当前视口（用于 chunk 选择）
    pub viewport: &'a OnionViewportUniform,
    /// wgpu 设备
    pub device: &'a wgpu::Device,
    /// wgpu 队列
    pub queue: &'a wgpu::Queue,
}

impl OnionRenderer {
    /// 上传洋葱皮音符到 GPU（只上传当前视口时间范围内覆盖的 chunk）
    ///
    /// # 数据流
    /// 1. OnionNoteList 已经把音符按 start_tick 排序并分块（CHUNK_SIZE=1M）
    /// 2. 每帧先 O(log chunks) 定位可能覆盖视口的 chunk，再哈希可见 chunk 集合
    /// 3. 若音符数据/颜色/可见 chunk 集合均未变化 → 直接跳过上传（零 CPU 扫描）
    /// 4. 否则只扫描可见 chunk，按精确时间重叠过滤，颜色注入后上传
    /// 5. Compute shader 在 GPU 端按 pitch / 当前音轨做精确剔除
    pub fn upload_notes(&mut self, params: OnionUploadParams<'_>) {
        // 1. 计算当前视口覆盖的 chunk 集合哈希（仅扫描 chunk 元数据，不碰音符）
        let mut chunk_hash = 0u64;
        if let Some(note_list) = params.note_list {
            let chunks = note_list.chunks();
            if !chunks.is_empty() && params.viewport.tick_end > params.viewport.tick_start {
                // 所有 tick_start < viewport.tick_end 的 chunk 都可能包含起始点在视口内的音符
                let end_idx = chunks
                    .partition_point(|c| (c.tick_start as f32) < params.viewport.tick_end);
                // 在这些候选 chunk 中，再按 tick_end > viewport.tick_start 精筛
                for (i, chunk) in chunks[..end_idx].iter().enumerate() {
                    if (chunk.tick_end as f32) > params.viewport.tick_start {
                        chunk_hash = chunk_hash
                            .wrapping_mul(31)
                            .wrapping_add((i as u64).wrapping_add(1));
                    }
                }
            }
        }

        // 2. Dirty tracking：note list / 颜色 / 可见 chunk 集合 都未变化 → 跳过
        if params.list_version == self.last_list_version
            && params.color_hash == self.last_color_hash
            && chunk_hash == self.last_chunk_hash
        {
            return;
        }

        // 3. 收集可见 chunk 中的音符，并注入颜色
        self.cpu_note_pool.clear();
        let mut dropped = 0usize;

        if let Some(note_list) = params.note_list {
            let notes = note_list.notes();
            let chunks = note_list.chunks();
            if !chunks.is_empty() && params.viewport.tick_end > params.viewport.tick_start {
                let end_idx = chunks
                    .partition_point(|c| (c.tick_start as f32) < params.viewport.tick_end);
                for chunk in &chunks[..end_idx] {
                    // chunk 的时间范围与视口无交集 → 跳过整个 chunk
                    if (chunk.tick_end as f32) <= params.viewport.tick_start {
                        continue;
                    }

                    for note in &notes[chunk.note_start..chunk.note_end] {
                        // 精确时间重叠过滤：只上传真正落在视口时间范围内的音符
                        if (note.end_tick() as f32) <= params.viewport.tick_start
                            || (note.start_tick() as f32) >= params.viewport.tick_end
                        {
                            continue;
                        }

                        if self.cpu_note_pool.len() >= self.max_capacity {
                            dropped += 1;
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
                }
            }
        }

        let uploaded = self.cpu_note_pool.len();
        self.total_note_count = uploaded as u32;

        if dropped > 0 {
            tracing::warn!(
                "Onion visible chunk capacity exceeded: dropped {} notes (capacity {})",
                dropped,
                self.max_capacity
            );
        }

        // 4. 按需扩容 note_pool
        let required = uploaded.max(Self::INITIAL_NOTE_CAPACITY);
        if required > self.note_pool_capacity {
            let new_cap = required
                .next_power_of_two()
                .min(self.max_capacity)
                .max(Self::INITIAL_NOTE_CAPACITY);
            if new_cap > self.note_pool_capacity {
                self.note_pool_buffer = Self::create_note_pool_buffer(params.device, new_cap);
                self.note_pool_capacity = new_cap;
                self.rebuild_bind_groups(params.device);
            }
        }

        // 5. 按需扩容 instance_indices
        let required_idx = uploaded.max(Self::INITIAL_INDICES_CAPACITY);
        if required_idx > self.indices_capacity {
            let new_cap = required_idx
                .next_power_of_two()
                .min(self.max_capacity)
                .max(Self::INITIAL_INDICES_CAPACITY);
            if new_cap > self.indices_capacity {
                self.instance_indices_buffer =
                    Self::create_instance_indices_buffer(params.device, new_cap);
                self.indices_capacity = new_cap;
                self.rebuild_bind_groups(params.device);
            }
        }

        let write_count = uploaded.min(self.note_pool_capacity);
        params.queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..write_count]),
        );

        self.last_list_version = params.list_version;
        self.last_color_hash = params.color_hash;
        self.last_chunk_hash = chunk_hash;
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
