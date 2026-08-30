use super::{ArrangementNoteInstance, ArrangementRenderer, ArrangementUniform};
use std::time::Instant;
use puffin;

impl ArrangementRenderer {
    /// 准备渲染数据
    ///
    /// - `overlay`：覆盖层实例（背景/lane/网格/框选/指示线），每帧重建，始终上传。
    /// - `notes`：`None` 表示音符实例未变化（常驻 GPU buffer），仅当音符数据/轨道顺序/
    ///   可见轨范围变化时为 `Some(...)` 才重新上传。着色器依据 uniform 完成视口变换与裁剪，
    ///   因此横向滚动只需更新 uniform，无需重建音符缓冲。
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uniform: ArrangementUniform,
        overlay: &[ArrangementNoteInstance],
        notes: Option<&[ArrangementNoteInstance]>,
        overlay_back_len: usize,
    ) {
        puffin::profile_scope!("arrangement::gpu_upload");
        let t0 = Instant::now();

        // 更新 uniform（滚动/缩放每帧变化）
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

        // 音符缓冲仅数据变化时上传
        if let Some(notes) = notes {
            let note_count = notes.len();
            let cap_t0 = Instant::now();
            Self::ensure_capacity(
                &mut self.note_buffer,
                &mut self.note_capacity,
                device,
                note_count,
            );
            let grow_ms = cap_t0.elapsed().as_secs_f64() * 1000.0;
            queue.write_buffer(self.note_buffer.inner(), 0, bytemuck::cast_slice(notes));
            let upload_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let bytes = note_count * std::mem::size_of::<ArrangementNoteInstance>();
            tracing::debug!(
                target: "perf::arrangement",
                instances = note_count,
                bytes,
                grow_ms,
                upload_ms,
                "gpu_upload_notes"
            );
            self.note_count = note_count as u32;
        }
    }
}
