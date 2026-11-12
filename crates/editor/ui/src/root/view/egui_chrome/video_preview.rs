//! 视频预览纹理：16:9 卡片 / 全屏瀑布流。
//!
//! 矩形不再从 iced `rect_probe` 读取，而是由 `root::video_clip_math` 直接计算：
//! - 渲染器入口：`renderer_panel_layout(content)` 的 16:9 预览黑盒；
//! - 全屏瀑布流：标题栏 + 左侧路由栏之外的整块区域。
//!
//! 同时把离屏纹理需要的尺寸写回 `waterfall_player.size`（存储 = 显示，同一事实源）。

use lumino_ui_egui::chrome::{RENDER_PREVIEW_TEXTURE_ID, TexturePreviewModel};

use crate::root::Root;
use crate::root::video_clip_math::{PreviewGate, preview_gate};
use crate::titlebar::mode_toggle::AppMode;

impl Root {
    /// 本帧的纹理预览模型；不可见或不应铺纹理时返回 `None`。
    ///
    /// `window` = 整窗逻辑矩形，`content` = 标题栏下、状态栏上的内容区。
    pub(crate) fn video_preview_model(
        &self,
        window: egui::Rect,
        content: egui::Rect,
    ) -> Option<TexturePreviewModel> {
        let waterfall_mode = self.state.current_mode == AppMode::Waterfall;
        let mut rects = Vec::new();

        if waterfall_mode {
            // 全屏瀑布流：标题栏 + 左侧路由栏之外铺满。
            let top = if self.settings.synth.use_native_titlebar && cfg!(target_os = "macos") {
                window.top()
            } else {
                window.top() + lumino_ui_egui::layout::TITLEBAR_H
            };
            let area = egui::Rect::from_min_max(
                egui::pos2(window.left() + self.sidebar.width() as f32, top),
                window.max,
            );
            self.waterfall_player
                .size
                .borrow_mut()
                .replace((area.width().max(1.0) as u32, area.height().max(1.0) as u32));
            if self.waterfall_player.view.is_some() {
                rects.push(area);
            }
        } else if self.is_renderer_entry_active() {
            let layout = crate::root::video_clip_math::renderer_panel_layout(content);
            self.waterfall_player.size.borrow_mut().replace((
                layout.preview.width().max(1.0) as u32,
                layout.preview.height().max(1.0) as u32,
            ));
            let texture_ready = self.waterfall_player.view.is_some();
            let in_band = self
                .state
                .video_clip
                .position_in_video_window(self.clip_real_duration_secs() as f32);
            if matches!(preview_gate(in_band, texture_ready), PreviewGate::Waterfall) {
                rects.push(layout.preview);
            }
        }

        if rects.is_empty() {
            return None;
        }
        Some(TexturePreviewModel {
            rects,
            texture: Some(RENDER_PREVIEW_TEXTURE_ID),
        })
    }
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

    fn content() -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(248.0, 30.0), egui::vec2(1800.0, 1182.0))
    }

    #[test]
    fn preview_gate_matrix_matches_iced_semantics() {
        assert_eq!(preview_gate(false, true), PreviewGate::NoMaterial);
        assert_eq!(preview_gate(false, false), PreviewGate::NoMaterial);
        assert_eq!(preview_gate(true, true), PreviewGate::Waterfall);
        assert_eq!(preview_gate(true, false), PreviewGate::WaitingRender);
    }

    #[test]
    fn renderer_preview_rect_matches_layout() {
        let mut root = root();
        // 进入渲染器入口（左侧栏 Renderer 分组且卷帘隐藏）。
        root.sidebar.active_group = Some(lumino_ui_core::sidebar_event::GroupId::Renderer);
        root.sidebar.piano_roll_visible = false;
        let layout = crate::root::video_clip_math::renderer_panel_layout(content());

        // 面板模型（无纹理也能出几何）：预览矩形与布局一致。
        let panel = root
            .renderer_panel_model(content())
            .expect("渲染器入口应给出面板模型");
        assert_eq!(panel.preview, layout.preview);
        assert!((layout.preview.width() / layout.preview.height() - 16.0 / 9.0).abs() < 0.01);

        // 纹理尺寸由模型构建时写回（存储 = 显示）。
        root.video_preview_model(
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(2048.0, 1232.0)),
            content(),
        );
        let (w, h) = root.waterfall_player.size.borrow().expect("应写回纹理尺寸");
        assert_eq!(
            (w, h),
            (
                layout.preview.width() as u32,
                layout.preview.height() as u32
            )
        );
    }

    #[test]
    fn waterfall_without_texture_keeps_placeholder() {
        let mut root = root();
        root.state.current_mode = AppMode::Waterfall;
        // 纹理未就绪 → 不铺纹理（露出 iced / 底色）。
        assert!(
            root.video_preview_model(
                egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(2048.0, 1232.0)),
                content(),
            )
            .is_none()
        );
    }
}
