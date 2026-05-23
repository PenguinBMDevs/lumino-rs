use super::{CameraUniform, OnionNote, OnionRenderer, OnionViewportUniform};

impl OnionRenderer {
    /// 准备计算剔除（视口或轨道掩码变化时调用）
    ///
    /// 执行 compute shader 剔除，结果写入 instance_indices_buffer 和 indirect_buffer。
    /// 内置 dirty tracking：当视口/相机/轨道掩码/音符均未变化时跳过 compute dispatch。
    /// `notes` 参数提供 CPU 端音符切片，用于二分查找定位可见范围，减少 GPU 扫描量。
    pub fn prepare_cull(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &OnionViewportUniform,
        camera: &CameraUniform,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        notes: Option<&[OnionNote]>,
    ) {
        if self.note_count == 0 {
            return;
        }

        // 构建完整的 viewport uniform（合并 cull 参数）
        let mut full_viewport = OnionViewportUniform {
            tick_start: viewport.tick_start,
            tick_end: viewport.tick_end,
            pitch_min: viewport.pitch_min,
            pitch_max: viewport.pitch_max,
            note_count: self.note_count as u32,
            indices_capacity: self.indices_capacity as u32,
            visible_start: 0,
            visible_end: self.note_count as u32,
        };

        // CPU 二分查找定位可见音符范围，GPU 跳过区间外的音符扫描
        if let Some(note_slice) = notes {
            full_viewport.fill_cull_range(note_slice);
        }

        // Dirty check：检测视口/相机/轨道掩码/音符是否有变化
        let viewport_changed = self.last_viewport.as_ref() != Some(&full_viewport);
        let camera_changed = self.last_camera.as_ref() != Some(camera);
        let anything_dirty = viewport_changed || camera_changed || self.notes_dirty;

        if !anything_dirty {
            return;
        }

        // 更新缓存状态
        self.last_viewport = Some(full_viewport);
        self.last_camera = Some(*camera);
        self.notes_dirty = false;

        // 上传视口 uniform，仅在变化时上传
        if viewport_changed || self.notes_dirty {
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[full_viewport]),
            );
        }
        // 上传相机 uniform，仅在变化时上传
        if camera_changed {
            queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[*camera]));
        }

        // 如果 bind group 因 buffer 重建而脏了，先修复
        if self.bind_groups_dirty {
            self.rebuild_bind_groups(device);
            self.bind_groups_dirty = false;
        }

        // 执行 Compute Culling — 仅派发可见范围内的 workgroup
        let cull_count = full_viewport.visible_end - full_viewport.visible_start;
        let workgroup_count = cull_count.div_ceil(Self::WORKGROUP_SIZE).max(1);
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("onion_cull_pass"),
            timestamp_writes: None,
        });
        compute_pass.set_pipeline(&self.compute_pipeline);
        compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);
        compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
    }

    /// 执行间接绘制（TriangleStrip + 4 顶点/实例，与 note.wgsl 一致）
    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>) {
        if self.note_count == 0 {
            return;
        }
        render_pass.set_pipeline(&self.render_pipeline);
        render_pass.set_bind_group(0, &self.render_bind_group, &[]);
        render_pass.draw_indirect(&self.indirect_buffer, 0);
    }
}
