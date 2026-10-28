//! 形状抽屉画布视图（DRAW-033B0/B1）
//!
//! B0 的空窗占位已替换为 B1 Canvas：左侧七工具 + 选择，右侧单位框画板。
//! 库列表/保存/插入仍由 B2 接入。

use iced_core::Length;
use iced_widget::canvas::Canvas;

use crate::shape_drawer::canvas::ShapeDrawerCanvas;
use crate::state::root_state::ShapeDrawerState;
use crate::{Element, Theme};

/// 渲染形状抽屉。
pub fn view_shape_drawer<'a>(state: &ShapeDrawerState, _theme: &'a Theme) -> Element<'a> {
    Canvas::new(ShapeDrawerCanvas {
        doc: state.doc.clone(),
        interaction: state.interaction.clone(),
        shapes_dir: if state.shapes_dir.as_os_str().is_empty() {
            "not injected".to_string()
        } else {
            state.shapes_dir.display().to_string()
        },
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
