//! 画布几何同步：egui 布局 → 编辑器 / 走带画布矩形。
//!
//! iced `Grid` program 曾在 `update()` 里发 `CanvasBoundsChanged`；egui 独占
//! 整窗后 iced 不再布局，改由本模块用与 egui 外壳完全同源的 insets 计算。
//! 几何只在变化时写回，避免无谓的状态变更与重绘。

use lumino_ui_egui::chrome::ChromeModel;
use lumino_ui_egui::layout;

use crate::host::Host;
use crate::message::{Message, Point2, Size2};

/// 浮点比较容差（亚像素差异不值得触发一次全量重算）。
const EPSILON: f32 = 0.01;

/// 画布几何是否由 egui 驱动。
///
/// **默认关闭**：iced `Grid` program 仍在 `update()` 里发 `CanvasBoundsChanged`，
/// 两个来源同时写会互相覆盖（每帧 ping-pong）。等 Canvas 输入迁到 egui、`Grid`
/// 被删除后，把它设为默认开启（或用 `LUMINO_EGUI_CANVAS_SYNC=1` 提前验证）。
fn canvas_sync_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_CANVAS_SYNC")
            .map(|value| value != "0")
            .unwrap_or(false)
    })
}

impl Host {
    /// 用本帧 egui 布局刷新编辑器画布几何。
    ///
    /// 纵向卷帘（布局不同，尚未迁移）与走带之外的特殊视图保持原样：返回
    /// `None` 时不写回，几何沿用上一帧 / iced 旧值。
    pub(crate) fn sync_canvas_bounds(&mut self, _model: &ChromeModel, window_area: egui::Rect) {
        if !canvas_sync_enabled() {
            return;
        }
        let Some((offset, size)) = self.canvas_geometry(window_area) else {
            return;
        };

        let canvas = &self.root.editor.editor_state.canvas;
        let changed = (canvas.offset_x - offset.0).abs() > EPSILON
            || (canvas.offset_y - offset.1).abs() > EPSILON
            || (canvas.size_x - size.0).abs() > EPSILON
            || (canvas.size_y - size.1).abs() > EPSILON;
        if !changed {
            return;
        }

        self.process_message(Message::CanvasBoundsChanged {
            offset: Point2::new(offset.0, offset.1),
            size: Size2::new(size.0, size.1),
        });
    }

    /// 本帧画布 `(offset, size)`；无法确定时返回 `None`。
    fn canvas_geometry(&self, window_area: egui::Rect) -> Option<((f32, f32), (f32, f32))> {
        if self.root.is_arrangement_mode() {
            let geometry = crate::host::render::arrangement_geometry::arrangement_canvas_geometry(
                window_area.width(),
                window_area.height(),
                self.root.sidebar.width() as f32,
                self.root.toolbar.height(),
                if cfg!(target_os = "macos") { 0.0 } else { 30.0 },
            );
            return Some((
                (geometry.offset.x, geometry.offset.y),
                (geometry.size.x, geometry.size.y),
            ));
        }

        // 纵向卷帘布局不同，尚未迁移：保持 iced 旧值。
        if self.root.sidebar.is_vertical_roll() {
            return None;
        }

        // insets 与 `Root::egui_piano_roll_model` 完全同源（见 `egui_chrome.rs`）。
        let sidebar = &self.root.sidebar;
        let left = 48.0
            + if sidebar.panel_visible {
                sidebar.panel_width
            } else {
                0.0
            };
        let right = if self.root.right_sidebar_visible() {
            48.0 + if self.root.right_sidebar.panel_visible {
                self.root.right_sidebar.panel_width
            } else {
                0.0
            }
        } else {
            0.0
        };
        let top = (if cfg!(target_os = "macos") && self.root.settings.synth.use_native_titlebar {
            0.0
        } else {
            layout::TITLEBAR_H
        }) + self.root.toolbar.height;
        let bottom = layout::STATUSBAR_H
            + if sidebar.automation_panel_visible {
                self.root.visual.velocity_panel_height
            } else {
                0.0
            };

        let width = (window_area.width() - left - right).max(1.0);
        let height = (window_area.height() - top - bottom).max(1.0);
        Some(((left, top), (width, height)))
    }
}
