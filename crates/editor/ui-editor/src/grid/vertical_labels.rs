//! 纵向卷帘小节号文本与边框（供 wgpu 网格接管后 Canvas 层调用）

use super::theme::ThemeExt;
use crate::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Path, Stroke};
use lumino_ui_core::Renderer;

/// 仅绘制小节号文本与边框（网格线已由 wgpu 绘制）
pub fn draw_labels(
    editor: &Editor,
    frame: &mut Frame<Renderer>,
    bounds: Rectangle,
    theme: &lumino_ui_core::Theme,
) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let ruler_height = view.ruler_height;
    let keyboard_h = view.keyboard_width;
    if bounds.height <= ruler_height + keyboard_h || bounds.width <= 1.0 {
        return;
    }
    let grid_top = ruler_height;
    let grid_bottom = bounds.height - keyboard_h;
    let grid_height = (grid_bottom - grid_top).max(0.0);
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + grid_height) / view.zoom_x;
    let ticks_per_measure = ppq * 4.0;
    if ticks_per_measure <= 0.0 {
        return;
    }
    let mut measure_tick = (start_tick / ticks_per_measure).ceil() * ticks_per_measure;
    let mut measure_no = (measure_tick / ticks_per_measure) as u32 + 1;
    let text_color = theme.text_color();
    while measure_tick < end_tick {
        let screen_y = grid_bottom - measure_tick * view.zoom_x + view.scroll_x;
        if screen_y >= grid_top && screen_y <= grid_bottom {
            let label = iced_widget::canvas::Text {
                content: measure_no.to_string(),
                position: Point::new(4.0, screen_y + 2.0),
                max_width: 60.0,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(10.0),
                color: text_color,
                font: iced_core::Font::DEFAULT,
                align_x: iced_core::alignment::Horizontal::Left.into(),
                align_y: iced_core::alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(label);
        }
        measure_tick += ticks_per_measure;
        measure_no += 1;
    }
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    let kb_top = Path::line(
        Point::new(0.0, grid_bottom),
        Point::new(bounds.width, grid_bottom),
    );
    frame.stroke(&kb_top, border_stroke);
    let ruler_bottom = Path::line(
        Point::new(0.0, grid_top),
        Point::new(bounds.width, grid_top),
    );
    frame.stroke(&ruler_bottom, border_stroke);
}
