use super::{
    ArrangementNoteInstance, ArrangementNoteUniform, ArrangementRenderer, ArrangementUniform,
};
use std::time::Instant;
use puffin;

impl ArrangementRenderer {
    /// 准备渲染数据
    ///
    /// 覆盖层（背景/lane/网格/框选/指示线）每帧重建并上传；
    /// 音符层**不再拥有第二份缓冲**——直接复用钢琴卷帘常驻 GPU 音符缓冲
    /// （`note_source`），仅上传走带专属的 uniform 与 `lane_index` 映射，
    /// 以及本帧可见音轨的 (offset, len) 分段。横向/纵向滚动只需更新 uniform，
    /// 无需重建任何音符数据（GPU 裁剪完成可视范围剔除）。
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: ArrangementUniform,
        overlay: &[ArrangementNoteInstance],
        overlay_back_len: usize,
        note_source: &wgpu::Buffer,
        note_uniform: ArrangementNoteUniform,
        lane_index: &[f32],
        note_segments: &[(u32, u32)],
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

        // ── 音符层：复用常驻 GPU 缓冲，仅上传 uniform + lane_index + 可见分段 ──
        let note_t0 = Instant::now();

        // 共享音符缓冲（GPU，按 NoteInstance 分段）；仅保存引用，绘制阶段 bind
        self.note_source = note_source.clone();
        self.note_segments = note_segments.to_vec();

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

        let upload_ms = note_t0.elapsed().as_secs_f64() * 1000.0;
        let visible_notes: u32 = note_segments.iter().map(|(_, len)| *len).sum();
        tracing::debug!(
            target: "perf::arrangement",
            visible_tracks = note_segments.len(),
            visible_notes,
            upload_ms,
            "gpu_upload_notes(reused_gpu_buffer)"
        );
    }
}
