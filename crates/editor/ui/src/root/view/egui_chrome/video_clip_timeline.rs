//! 视频剪辑时间轴：生产状态 → egui 模型 + 真实矩形。
//!
//! 矩形不按常量正推（预览卡会被收缩/居中，实测差 ≈172 逻辑像素），
//! 而是读 `rect_probe` 在 iced 布局里记录的真实 bounds。

use lumino_ui_egui::video_clip_timeline::{ClipSpan, VideoTimelineModel};

use crate::root::Root;

impl Root {
    /// 渲染器入口面板里的时间轴模型（含真实矩形）；不在该面板时返回 `None`。
    pub(crate) fn video_timeline_model(&self) -> Option<(egui::Rect, VideoTimelineModel)> {
        if !self.is_renderer_entry_active() {
            return None;
        }
        let Some(bounds) = crate::view::video_clip::rect_probe::last_rect("renderer::clip_canvas")
        else {
            tracing::warn!("DEBUG clip: no rect");
            return None;
        };
        if bounds.width <= 0.0 || bounds.height <= 0.0 {
            return None;
        }

        let total_ticks = self.clip_real_total_ticks();
        let ppq = self.editor.editor_state.view.ppq;
        let tempos = self.tempo_pairs();
        let duration_secs =
            crate::view::video_clip::timeline::duration_seconds(total_ticks, ppq, &tempos) as f32;
        let clip = &self.state.video_clip;

        let span = |edit: &lumino_ui_core::state::video_clip_state::ClipTrackEdit| ClipSpan {
            start: edit.visible_start(),
            len: edit.visible_len(duration_secs),
        };

        Some((
            egui::Rect::from_min_size(
                egui::pos2(bounds.x, bounds.y),
                egui::vec2(bounds.width, bounds.height),
            ),
            VideoTimelineModel {
                duration_secs,
                zoom: clip.zoom,
                scroll_x: clip.timeline_scroll_x,
                playhead_secs: clip.clip_position_secs,
                video: span(&clip.video_edit),
                audio: span(&clip.audio_edit),
            },
        ))
    }
}
