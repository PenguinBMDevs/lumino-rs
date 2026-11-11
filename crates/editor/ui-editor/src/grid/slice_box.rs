//! DRAW-025：切割线预览
//!
//! 布防后拖拽中实时画一条高对比直线；松手即按该线切分。

use iced_core::{Color, Rectangle};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;

use crate::Editor;

/// 切割线颜色（橙红，亮/暗主题下都可见）
const CUT_COLOUR: Color = Color {
    r: 1.0,
    g: 0.35,
    b: 0.25,
    a: 0.9,
};

/// 绘制正在拖拽的切割线；未布防或未拖拽时返回 `None`
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Option<Geometry<Renderer>> {
    let state = editor.slice_cut?;
    if !state.dragging {
        return None;
    }
    let from = editor.line_pos_screen_pos(state.from);
    let to = editor.line_pos_screen_pos(state.to);
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    frame.stroke(
        &Path::line(from, to),
        Stroke::default().with_width(2.0).with_color(CUT_COLOUR),
    );
    for point in [from, to] {
        frame.fill(&Path::circle(point, 3.0), CUT_COLOUR);
    }
    Some(frame.into_geometry())
}
