//! 纹理预览（渲染器面板 16:9 卡片 / 全屏瀑布流）：生产状态 → egui 模型。
//!
//! 两块区域互斥：`AppMode::Waterfall` 时用全屏播放器矩形，否则在渲染器入口
//! 用 16:9 预览卡片矩形（都由 iced 布局探针给出**实测**坐标）。
//! 纹理只在门控判定「该显示瀑布流」时才交给 egui——否则保留 iced 占位文本。

use lumino_ui_egui::chrome::{RENDER_PREVIEW_TEXTURE_ID, TexturePreviewModel};

use crate::root::Root;
use crate::titlebar::mode_toggle::AppMode;
use crate::view::video_clip::preview::{PreviewGate, preview_gate};
use crate::view::video_clip::rect_probe;

impl Root {
    /// 本帧的纹理预览模型；两块区域都不可见时返回 `None`。
    pub(crate) fn video_preview_model(&self) -> Option<TexturePreviewModel> {
        let waterfall_mode = self.state.current_mode == AppMode::Waterfall;
        let mut rects = Vec::new();
        if waterfall_mode {
            if let Some(rect) = rect_probe::last_rect("waterfall::player") {
                rects.push(to_egui_rect(rect));
            }
        } else if self.is_renderer_entry_active()
            && let Some(rect) = rect_probe::last_rect("renderer::preview")
        {
            rects.push(to_egui_rect(rect));
        }
        if rects.is_empty() {
            return None;
        }

        // 门控与 iced 侧同源：先判「播放头在不在素材带上」，再判纹理是否就绪。
        let texture_ready = self.waterfall_player.view.is_some();
        let show = if waterfall_mode {
            texture_ready
        } else {
            let in_band = self
                .state
                .video_clip
                .position_in_video_window(self.clip_real_duration_secs() as f32);
            matches!(preview_gate(in_band, texture_ready), PreviewGate::Waterfall)
        };
        Some(TexturePreviewModel {
            rects,
            texture: show.then_some(RENDER_PREVIEW_TEXTURE_ID),
        })
    }
}

fn to_egui_rect(rect: iced_core::Rectangle) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(rect.x, rect.y),
        egui::vec2(rect.width, rect.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::root_state::DialogType;

    fn root() -> Root {
        Root::new_dialog(
            &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
            DialogType::VideoExport,
        )
    }

    #[test]
    fn no_probe_rect_means_no_model() {
        // 无头环境没有 iced 布局探针 → 不产出模型（不凭空画图）。
        assert!(root().video_preview_model().is_none());
    }

    #[test]
    fn waterfall_mode_without_rect_keeps_placeholder() {
        let mut root = root();
        root.state.current_mode = AppMode::Waterfall;
        // 无探针矩形 → 仍是 None（先决条件是布局实测）。
        assert!(root.video_preview_model().is_none());
    }
}
