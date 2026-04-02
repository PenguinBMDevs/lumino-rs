//! 选择框渲染

use crate::Renderer;
use crate::constants::editor::SELECTION_BOX_FILL_ALPHA;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};

/// 绘制选择框
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &crate::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let (start_pos, current_pos) = editor.get_selection_box()?;

    // 计算选择框的位置和尺寸
    let min_x = start_pos.x.min(current_pos.x);
    let max_x = start_pos.x.max(current_pos.x);
    let min_y = start_pos.y.min(current_pos.y);
    let max_y = start_pos.y.max(current_pos.y);

    let width = max_x - min_x;
    let height = max_y - min_y;

    // 最小尺寸检查
    if width < 1.0 || height < 1.0 {
        return None;
    }

    let palette = theme.extended_palette();
    let selection_color = palette.secondary.strong.color;

    let mut frame = canvas::Frame::new(renderer, bounds.size());

    // 绘制填充（半透明）
    let rect = Rectangle::new(Point::new(min_x, min_y), Size::new(width, height));
    let path = Path::rectangle(rect.position(), rect.size());

    let fill_color = iced_core::Color {
        r: selection_color.r,
        g: selection_color.g,
        b: selection_color.b,
        a: SELECTION_BOX_FILL_ALPHA,
    };
    frame.fill(&path, fill_color);

    // 绘制边框
    let stroke = Stroke::default()
        .with_width(1.0)
        .with_color(selection_color);
    frame.stroke(&path, stroke);

    Some(frame.into_geometry())
}
