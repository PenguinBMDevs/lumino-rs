use super::{CameraUniform, DrawIndirectArgs, OnionKeyRange, OnionRenderer, OnionViewportUniform};
use crate::OnionSkinBucket;

impl OnionRenderer {
    /// 准备计算剔除（视口变化时调用）
    ///
    /// 执行 compute shader 剔除，结果写入 instance_indices_buffer 和 indirect_buffer。
    /// 内置 dirty tracking：当视口/相机/音符均未变化时跳过 compute dispatch。
    ///
    /// # Bucket 模式
    /// 如果 `bucket` 不为 None，使用 GPU 常驻音符池 + per-key 可见范围：
    /// - CPU 端对每个可见 key 做二分查找，得到 `[start, end)`；
    /// - 只上传 256 个 `OnionKeyRange`（约 2KB），替代原先最多 3M 音符的上传；
    /// - GPU 每个 workgroup 处理一个 key，只扫描该 key 的可见子区间。
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_cull(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &OnionViewportUniform,
        camera: &CameraUniform,
        queue: &wgpu::Queue,
        device: &wgpu::Device,
        bucket: Option<&OnionSkinBucket>,
        current_track: u16,
    ) {
        if self.note_count == 0 {
            return;
        }

        let mut full_viewport = *viewport;
        full_viewport.current_track = current_track as u32;

        let mut key_ranges = [OnionKeyRange::default(); 256];

        if self.bucket_mode {
            full_viewport.use_key_ranges = 1;
            // 二分查找每个 key 的可见范围，并映射到 uploaded pool 的坐标空间
            let ts = viewport.tick_start as u32;
            let te = viewport.tick_end as u32;
            let key_min = viewport.pitch_min.max(0.0) as u16;
            let key_max = viewport.pitch_max.min(255.0) as u16;
            let b = bucket.expect("bucket_mode requires Some(bucket)");
            for key in key_min..=key_max {
                // 当前视口的 visible range（在 full bucket 坐标系中）
                let (curr_start, curr_end) = b.find_visible_range(key as u8, ts, te);
                // upload 时保存的 range（在 full bucket 坐标系中）
                let upload = &self.upload_key_ranges[key as usize];
                // 取交集：overlap 在 uploaded pool 中的 local 索引
                let overlap_start = curr_start.max(upload.start as usize);
                let overlap_end = curr_end.min(upload.end as usize);
                if overlap_start < overlap_end {
                    key_ranges[key as usize] = OnionKeyRange {
                        start: (overlap_start - upload.start as usize) as u32,
                        end: (overlap_end - upload.start as usize) as u32,
                    };
                }
                // 无交集：uploaded 数据未覆盖当前视口，触发下次 upload 刷新
                // （Runner 中的 no_change 检测会捕捉到 tick range 变化并重新 upload）
            }
        } else {
            full_viewport.use_key_ranges = 0;
            full_viewport.visible_start = 0;
            full_viewport.visible_end = self.note_count as u32;
        }

        // Dirty check：检测视口/相机/音符是否有变化
        let viewport_changed = self.last_viewport.as_ref() != Some(&full_viewport);
        let camera_changed = self.last_camera.as_ref() != Some(camera);
        let anything_dirty = viewport_changed || camera_changed || self.notes_dirty;

        if !anything_dirty {
            return;
        }

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
        // Bucket 模式：每帧上传 key_ranges
        if self.bucket_mode {
            queue.write_buffer(
                &self.key_ranges_buffer,
                0,
                bytemuck::cast_slice(&key_ranges),
            );
        }

        // 更新缓存状态
        self.last_viewport = Some(full_viewport);
        self.last_camera = Some(*camera);
        self.notes_dirty = false;

        // 如果 bind group 因 buffer 重建而脏了，先修复
        if self.bind_groups_dirty {
            self.rebuild_bind_groups(device);
            self.bind_groups_dirty = false;
        }

        // CPU 端重置 indirect args（vertex_count=4, instance_count=0）
        // 替代原有的 GPU 端 `global_id.x == 0u` 重置方式，消除多 workgroup 并行竞态。
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

        // 执行 Compute Culling
        // Bucket 模式：固定 256 个 workgroup，每个处理一个 key
        // 兼容模式：按 visible_end - visible_start 计算
        let workgroup_count = if self.bucket_mode {
            256u32
        } else {
            full_viewport
                .visible_end
                .saturating_sub(full_viewport.visible_start)
                .div_ceil(Self::WORKGROUP_SIZE)
                .max(1)
        };
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
