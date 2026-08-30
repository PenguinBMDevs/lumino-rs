use super::{
    ArrangementNoteInstance, ArrangementNoteUniform, ArrangementRenderer, ArrangementUniform,
};
use crate::note_renderer::types::{CullUniform, DrawIndirectArgs};
use std::time::Instant;
use puffin;

impl ArrangementRenderer {
    /// 准备渲染数据
    ///
    /// 覆盖层（背景/lane/网格/框选/指示线）每帧重建并上传；
    /// 音符层**不再拥有第二份缓冲**——直接复用钢琴卷帘常驻 GPU 音符缓冲
    /// （`note_source`），仅上传走带专属的 uniform 与 `lane_index` 映射，
    /// 并用 `cull` 计算着色器在 GPU 上完成可视范围裁剪（消除此前每帧 ~67ms
    /// 的 CPU 逐音符重建）。绘制阶段由 `run_cull` + `draw` 配合 `draw_indirect`
    /// 一次性提交可见音符，CPU 零参与。
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: ArrangementUniform,
        overlay: &[ArrangementNoteInstance],
        overlay_back_len: usize,
        note_source: &wgpu::Buffer,
        note_instance_count: u32,
        note_uniform: ArrangementNoteUniform,
        lane_index: &[f32],
    ) {
        puffin::profile_scope!("arrangement::gpu_upload");
        let t0 = Instant::now();

        // 覆盖层 uniform（滚动/缩放每帧变化）
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[uniform]),
        );

        // 覆盖层每帧重建并上传
        let overlay_count = overlay.len();
        if overlay_count > 0 {
            let cap_t0 = Instant::now();
            Self::ensure_capacity(
                &mut self.overlay_buffer,
                &mut self.overlay_capacity,
                device,
                overlay_count,
            );
            let grow_ms = cap_t0.elapsed().as_secs_f64() * 1000.0;
            queue.write_buffer(
                self.overlay_buffer.inner(),
                0,
                bytemuck::cast_slice(overlay),
            );
            let upload_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let bytes = overlay_count * std::mem::size_of::<ArrangementNoteInstance>();
            tracing::debug!(
                target: "perf::arrangement",
                instances = overlay_count,
                bytes,
                grow_ms,
                upload_ms,
                "gpu_upload_overlay"
            );
        }
        self.overlay_count = overlay_count as u32;
        self.overlay_back_len = overlay_back_len as u32;

        // ── 音符层：复用常驻 GPU 缓冲，GPU 裁剪 + draw_indirect ──
        let note_t0 = Instant::now();

        // 共享音符缓冲（GPU，按 NoteInstance 分段）；保存引用
        self.note_source = note_source.clone();
        self.note_instance_count = note_instance_count;

        // lane_index：文档音轨 → 泳道序号（随侧栏排序变化才刷新，通常逐帧重传开销极小）
        if !lane_index.is_empty() {
            Self::ensure_lane_capacity(
                &mut self.lane_index_buffer,
                &mut self.lane_index_capacity,
                device,
                lane_index.len(),
            );
            queue.write_buffer(
                self.lane_index_buffer.inner(),
                0,
                bytemuck::cast_slice(lane_index),
            );
        }

        // 音符着色器 uniform（滚动/缩放/泳道高/画布偏移）
        queue.write_buffer(
            self.note_uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[note_uniform]),
        );

        // 可见索引缓冲容量：至少容纳全部实例的 u32 索引（cull 输出全局源索引）。
        // 必须在重建 bind group 之前扩容，否则 bind group 会绑定到过小的旧缓冲。
        let required_visible = (note_instance_count as u64)
            * std::mem::size_of::<u32>() as u64;
        Self::ensure_visible_capacity(&mut self.note_visible_buffer, device, required_visible);

        // 重建裁剪/绘制 bind group —— 两个 bind group 都按整份 `all_instances`
        // （共享的钢琴卷帘常驻缓冲）绑定，cull 输出全局源索引。每帧重建成本极低
        // （仅 2 个 bind group），但可同时覆盖首帧与 onion 缓冲 / 可见缓冲扩容的情况，
        // 保证 bind group 永远引用到当前最新大小的缓冲。
        self.rebuild_note_bind_groups(device);

        // 裁剪 uniform：单次全局分发覆盖整份缓冲（chunk_start=0, chunk_count=总数）
        let cull_uniform = CullUniform {
            instance_count: note_instance_count,
            chunk_start: 0,
            chunk_count: note_instance_count,
            _padding: 0,
        };
        queue.write_buffer(
            self.cull_info_buffer.inner(),
            0,
            bytemuck::cast_slice(&[cull_uniform]),
        );

        // 重置间接绘制参数（instance_count=0，cull 阶段原子累加可见数）
        queue.write_buffer(
            self.note_indirect_buffer.inner(),
            0,
            bytemuck::cast_slice(&[DrawIndirectArgs::default()]),
        );

        let upload_ms = note_t0.elapsed().as_secs_f64() * 1000.0;
        tracing::debug!(
            target: "perf::arrangement",
            visible_tracks = lane_index.len(),
            note_instance_count,
            upload_ms,
            "gpu_upload_notes(reused_gpu_buffer)"
        );
    }

    /// 重建音符裁剪 / 绘制 bind group（共享缓冲句柄变化时调用）
    ///
    /// 两个 bind group 都按整份 `all_instances`（即共享的钢琴卷帘常驻缓冲）绑定，
    /// cull 阶段输出全局源索引，绘制阶段用 `visible_index` 直接从该缓冲回查。
    fn rebuild_note_bind_groups(&mut self, device: &wgpu::Device) {
        let source = self.note_source.as_entire_binding();

        // 绘制 bind group：uniform + lane_index 存储 + 全部实例存储
        self.note_draw_bind_group =
            Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("arrangement_note_draw_bind_group"),
                layout: &self.note_draw_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.note_uniform_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.lane_index_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: source.clone(),
                    },
                ],
            }));

        // 裁剪 bind group：uniform + cull_info + 实例 + lane_index + 可见索引 + indirect
        self.note_cull_bind_group =
            Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("arrangement_note_cull_bind_group"),
                layout: &self.note_cull_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.note_uniform_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.cull_info_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: source,
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.lane_index_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.note_visible_buffer.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.note_indirect_buffer.inner().as_entire_binding(),
                    },
                ],
            }));
    }
}


