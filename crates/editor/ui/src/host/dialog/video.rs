//! Host 视频导出相关方法（`impl Host`）
//!
//! 从 `host/dialog.rs` 拆分而来：进度/预览帧/导出完成/失败的状态更新。

use crate::host::Host;
use crate::state::root_state::VideoExportOverlayState;

impl Host {
    /// 更新视频导出进度
    pub fn update_video_export_progress(
        &mut self,
        message: String,
        progress: f64,
        total_frames: u64,
        render_fps: f64,
        elapsed_secs: f64,
    ) {
        let dialog_state = &mut self.root.state.video_export_dialog;
        // 如果 overlay 尚未激活（e.g. 对话框窗口刚打开时），触发 Exporting 状态
        if matches!(dialog_state.overlay, VideoExportOverlayState::None) {
            dialog_state.overlay = VideoExportOverlayState::Exporting;
        }
        dialog_state.status_message = message;
        dialog_state.progress = progress;
        dialog_state.total_frames = total_frames;
        dialog_state.render_fps = render_fps;
        dialog_state.elapsed_secs = elapsed_secs;
        dialog_state.current_frame =
            (progress * total_frames as f64).min(total_frames as f64) as u64;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新视频导出预览帧
    pub fn update_video_export_preview_frame(&mut self, data: Vec<u8>, width: u32, height: u32) {
        let dialog_state = &mut self.root.state.video_export_dialog;
        let expected_len = (width * height * 4) as usize;
        if data.len() != expected_len {
            tracing::warn!(
                "视频导出预览帧尺寸不匹配: {}x{} 期望 {} bytes, 实际 {} bytes",
                width,
                height,
                expected_len,
                data.len()
            );
        }

        // 仅在数据变化时创建新 handle，避免每帧生成唯一 ID 导致 GPU 缓存失效
        let data_changed = dialog_state.preview_frame.as_deref() != Some(data.as_slice())
            || dialog_state.preview_width != width
            || dialog_state.preview_height != height;

        if data_changed {
            dialog_state.cached_image_handle = Some(iced_core::image::Handle::from_rgba(
                width,
                height,
                data.clone(),
            ));
        }
        // 即使数据未变，也更新 frame 用于 view 中的尺寸判断
        dialog_state.preview_frame = Some(data);
        dialog_state.preview_width = width;
        dialog_state.preview_height = height;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 检查视频导出是否正在进行
    pub fn is_video_exporting(&self) -> bool {
        matches!(
            self.root.state.video_export_dialog.overlay,
            VideoExportOverlayState::Exporting
        )
    }

    /// 标记视频导出完成
    pub fn set_video_export_completed(&mut self, elapsed_secs: f64) {
        let dialog_state = &mut self.root.state.video_export_dialog;
        let total_frames = dialog_state.total_frames;
        dialog_state.overlay = VideoExportOverlayState::Completed {
            total_frames,
            elapsed_secs,
            avg_fps: dialog_state.render_fps,
        };
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 标记视频导出失败
    pub fn set_video_export_failed(&mut self, error: String) {
        self.root.state.video_export_dialog.overlay = VideoExportOverlayState::Error(error);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
