//! 视频剪辑时间轴：生产状态 → egui 模型 + 真实矩形。
//!
//! 矩形不按常量正推（预览卡会被收缩/居中，实测差 ≈172 逻辑像素），
//! 而是读 `rect_probe` 在 iced 布局里记录的真实 bounds。

use lumino_message::video_clip::{ClipTrack, ClipTrimEdge, VideoClipAction};
use lumino_ui_egui::video_clip_timeline::{
    ClipEdge, ClipEdit, VideoTimelineAction, VideoTimelineModel,
};

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

        let clip_edit = |edit: &lumino_ui_core::state::video_clip_state::ClipTrackEdit| ClipEdit {
            offset: edit.offset_secs,
            trim_in: edit.trim_in_secs,
            trim_out: edit.trim_out_secs,
        };

        // 标题行矩形：内容（标题 / 传输按钮 / 信息）已全部由 egui 自绘。
        let title_row = crate::view::video_clip::rect_probe::last_rect("renderer::timeline_title")
            .map(|rect| {
                egui::Rect::from_min_size(
                    egui::pos2(rect.x, rect.y),
                    egui::vec2(rect.width, rect.height),
                )
            });
        let language = self.settings.display.language;
        let title = lumino_core::tr!("剪辑带").to_string();
        let info = format!(
            "时长 {:.1}s  位置 {:.1}s  缩放 {:.1}x   {}",
            duration_secs,
            clip.clip_position_secs,
            clip.zoom,
            lumino_core::tr!("空格 播放/暂停")
        );
        let _ = language;
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
                video: clip_edit(&clip.video_edit),
                audio: clip_edit(&clip.audio_edit),
                viewport_w: bounds.width,
                title_row,
                title,
                info,
                playing: clip.clip_playing,
            },
        ))
    }
}

impl Root {
    /// 时间轴动作 → `VideoClipAction`（拖动/裁剪均为**绝对值**，与 iced 同语义）。
    pub(crate) fn video_timeline_action_message(
        &self,
        action: &VideoTimelineAction,
        viewport_w: f32,
    ) -> crate::Message {
        let track = |t: lumino_ui_egui::video_clip_timeline::ClipTrack| match t {
            lumino_ui_egui::video_clip_timeline::ClipTrack::Video => ClipTrack::Video,
            lumino_ui_egui::video_clip_timeline::ClipTrack::Audio => ClipTrack::Audio,
        };
        let clip = &self.state.video_clip;
        let action = match action {
            VideoTimelineAction::Seek(secs) => VideoClipAction::TimelineSeek { secs: *secs },
            VideoTimelineAction::SetOffset { track: t, offset } => {
                VideoClipAction::ClipTrackOffsetChanged {
                    track: track(*t),
                    offset_secs: *offset,
                }
            }
            VideoTimelineAction::SetTrim {
                track: t,
                edge,
                secs,
            } => VideoClipAction::ClipTrimChanged {
                track: track(*t),
                edge: match edge {
                    ClipEdge::Start => ClipTrimEdge::Start,
                    ClipEdge::End => ClipTrimEdge::End,
                },
                trim_secs: *secs,
            },
            VideoTimelineAction::Zoom {
                factor,
                fixed_ratio,
            } => VideoClipAction::TimelineZoom {
                zoom: clip.zoom * factor,
                fixed_ratio: *fixed_ratio,
                viewport_w,
            },
            VideoTimelineAction::ScrollX(dx) => VideoClipAction::TimelineScroll {
                x: clip.timeline_scroll_x + dx,
                viewport_w,
            },
            VideoTimelineAction::PlayToggled => VideoClipAction::ClipPlayToggled,
            VideoTimelineAction::Rewound => VideoClipAction::ClipRewound,
        };
        crate::Message::VideoClip(action)
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use crate::state::root_state::DialogType;

    fn root() -> Root {
        Root::new_dialog(
            &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
            DialogType::VideoExport,
        )
    }

    #[test]
    fn transport_actions_map_to_video_clip_messages() {
        let root = root();
        for (action, ok) in [
            (
                VideoTimelineAction::PlayToggled,
                matches!(
                    root.video_timeline_action_message(&VideoTimelineAction::PlayToggled, 100.0),
                    crate::Message::VideoClip(VideoClipAction::ClipPlayToggled)
                ),
            ),
            (
                VideoTimelineAction::Rewound,
                matches!(
                    root.video_timeline_action_message(&VideoTimelineAction::Rewound, 100.0),
                    crate::Message::VideoClip(VideoClipAction::ClipRewound)
                ),
            ),
        ] {
            let _ = action;
            assert!(ok);
        }
    }
}
