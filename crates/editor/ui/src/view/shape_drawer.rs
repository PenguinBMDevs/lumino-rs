//! 形状抽屉视图（DRAW-033B0/B1/B2）
//!
//! 左侧是形状库与文件/变换/插入按钮；右侧是 B1 Canvas 画板。

use iced_core::{Background, Length};
use iced_widget::{container, space};

use crate::Theme;

/// 渲染形状抽屉（整窗占位）。
///
/// 面板 / 分割线 / 画布全部由 egui 绘制（`ui-egui/src/shape_drawer.rs` +
/// `shape_canvas.rs`，事件逻辑在 `shape_drawer::canvas::input` 与 iced 共用）。
/// iced 只保留同尺寸底色，保证窗口测量与拖拽手感不变。
pub fn view_shape_drawer<'a>(
    _state: &crate::state::root_state::ShapeDrawerState,
    theme: &'a Theme,
) -> crate::Element<'a> {
    let bg = theme.extended_palette().background.base.color;
    container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            ..Default::default()
        })
        .into()
}
