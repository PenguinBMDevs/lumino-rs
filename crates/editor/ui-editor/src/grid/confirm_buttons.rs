//! 悬浮 √× 确认/取消按钮的共享几何。
//!
//! 视觉与点击由 egui 覆盖层绘制（`ui-egui::editor_action_buttons` 等）；
//! 本模块只保留生产侧与 egui 模型共用的尺寸常量与矩形换算。

use iced_core::Rectangle;

/// 悬浮按钮边长（用户要求 30-40 像素，取 36）。
pub const BUTTON_SIZE: f32 = 36.0;

/// 按钮圆角半径。
pub const BUTTON_RADIUS: f32 = 8.0;

/// `iced_core::Rectangle` → `(x, y, width, height)`。
///
/// 供不依赖 iced 的 UI 层（egui 悬浮按钮）复用同一份几何。
pub fn rect_tuple(rect: Rectangle) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.width, rect.height)
}
