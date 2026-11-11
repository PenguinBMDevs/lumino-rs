//! DRAW-025：饼干模「刀」的可视化
//!
//! 切到鼠标工具时选中的手画音符会被快照成刀；这里用**青色虚线框**把它们标出来，
//! 避免用户「不知道刀到底拿没拿到」。刀被消费（饼干模成功）后自动消失。

use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_core::Renderer;

use crate::Editor;

/// 刀的颜色（青色，与切割线的橙红区分）
const KNIFE_COLOUR: Color = Color {
    r: 0.20,
    g: 0.78,
    b: 0.86,
    a: 0.95,
};

/// 虚线段长 / 间隔（屏幕像素）
const DASH: f32 = 5.0;
const GAP: f32 = 3.0;
/// 线宽
const WIDTH: f32 = 1.5;

/// 绘制当前「刀」的虚线框；没有刀时返回 `None`
pub fn draw(editor: &Editor, renderer: &Renderer, bounds: Rectangle) -> Option<Geometry<Renderer>> {
    let rects = knife_rects(editor);
    if rects.is_empty() {
        return None;
    }
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    for rect in rects {
        dashed_rect(&mut frame, rect);
    }
    Some(frame.into_geometry())
}

/// 当前「刀」在屏幕空间的矩形（每条音符一个；无刀时为空）
///
/// 横向卷帘：音符行占 `[key_to_y(key), key_to_y(key) + zoom_y]`。
/// **不能用 `key + 1` 求下边界**——屏幕 y 向下增大而 key 向上增大，
/// `key + 1` 得到的是上一行的顶边，整个框会上移一行（DRAW-025 实测）。
pub(crate) fn knife_rects(editor: &Editor) -> Vec<Rectangle> {
    let Some(walls) = editor.mould_walls.as_ref() else {
        return Vec::new();
    };
    walls.iter().map(|note| note_rect(editor, note)).collect()
}

/// 单条音符的屏幕矩形（与音符绘制同口径）
fn note_rect(editor: &Editor, note: &lumino_editor_state::ShapeNote) -> Rectangle {
    let state = &editor.editor_state;
    let view = &state.view;
    if state.is_vertical_roll {
        // 纵向：tick→Y（头部在底部、向上增长），key→X
        let grid_bottom = state.canvas.size_y - view.keyboard_width;
        let x = note.key as f32 * view.zoom_y - view.scroll_y;
        let y = grid_bottom - (note.tick + note.length) * view.zoom_x + view.scroll_x;
        Rectangle::new(
            Point::new(x, y),
            Size::new(view.zoom_y, (note.length * view.zoom_x).max(1.0)),
        )
    } else {
        let x = view.tick_to_x(note.tick);
        let y = view.key_to_y(note.key);
        Rectangle::new(
            Point::new(x, y),
            Size::new((note.length * view.zoom_x).max(1.0), view.zoom_y),
        )
    }
}

/// 四条边都按虚线画
fn dashed_rect(frame: &mut canvas::Frame<Renderer>, rect: Rectangle) {
    let corners = [
        rect.position(),
        Point::new(rect.x + rect.width, rect.y),
        Point::new(rect.x + rect.width, rect.y + rect.height),
        Point::new(rect.x, rect.y + rect.height),
    ];
    for index in 0..4 {
        dashed_line(frame, corners[index], corners[(index + 1) % 4]);
    }
}

/// 单条虚线段
fn dashed_line(frame: &mut canvas::Frame<Renderer>, from: Point, to: Point) {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.5 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let mut cursor = 0.0;
    while cursor < len {
        let end = (cursor + DASH).min(len);
        frame.stroke(
            &Path::line(
                Point::new(from.x + ux * cursor, from.y + uy * cursor),
                Point::new(from.x + ux * end, from.y + uy * end),
            ),
            Stroke::default().with_width(WIDTH).with_color(KNIFE_COLOUR),
        );
        cursor = end + GAP;
    }
}

#[cfg(test)]
mod tests;
