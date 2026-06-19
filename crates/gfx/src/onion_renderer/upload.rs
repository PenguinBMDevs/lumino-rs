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
    /// wgpu 设备
    pub device: &'a wgpu::Device,
    /// wgpu 队列
    pub queue: &'a wgpu::Queue,
}

impl OnionRenderer {
    /// 上传洋葱皮音符到 GPU（全量常驻 + 颜色注入）
    ///
    /// # 数据流
    /// 1. CPU 只在上游数据/颜色变化时执行一次：颜色注入 + write_buffer
    /// 2. 音符数据常驻 GPU storage buffer（按 GPU 最大容量截取）
    /// 3. 滚动/缩放时不再触发任何 CPU 过滤或重传，只更新 viewport uniform
    /// 4. Compute shader 在 GPU 端做视口剔除
    pub fn upload_notes(&mut self, params: OnionUploadParams<'_>) {
        // Dirty tracking：note list / 颜色 都未变化 → 跳过
        if params.list_version == self.last_list_version
            && params.color_hash == self.last_color_hash
        {
            return;
        }

        let source_count = params.notes.len();
        let upload_count = source_count.min(self.max_capacity);

        if source_count > self.max_capacity {
            tracing::warn!(
                "Onion note pool capacity exceeded: source {} notes, max_capacity {}, \
                 uploaded {} notes. 后续音轨将不可见；如需完整显示请使用显存更大的 GPU。",
                source_count,
                self.max_capacity,
                upload_count
            );
        }

        self.total_note_count = upload_count as u32;

        // 颜色注入 + CPU 缓存
        self.cpu_note_pool.clear();
        self.cpu_note_pool.reserve(upload_count);
        if params.track_colors.is_empty() {
            self.cpu_note_pool
                .extend_from_slice(&params.notes[..upload_count]);
        } else {
            for note in &params.notes[..upload_count] {
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

        // 按需扩容 note_pool
        let required = upload_count.max(Self::INITIAL_NOTE_CAPACITY);
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

        // 按需扩容 instance_indices：与 note_pool 同容量即可覆盖
        // “所有音符同时可见”的最坏情况
        let required_idx = upload_count.max(Self::INITIAL_INDICES_CAPACITY);
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

        let write_count = upload_count.min(self.note_pool_capacity);
        params.queue.write_buffer(
            &self.note_pool_buffer,
            0,
            bytemuck::cast_slice(&self.cpu_note_pool[..write_count]),
        );

        self.last_list_version = params.list_version;
        self.last_color_hash = params.color_hash;
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
