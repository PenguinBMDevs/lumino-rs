//! 小节线/网格线绘制

use super::theme::ThemeExt;
use crate::Renderer;
use crate::editor::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::{Frame, Path, Stroke};

/// 绘制小节线和拍线（纵向线）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let view = &editor.state;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let measure_ticks = ppq * 4.0;
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

    // 网格线间隔：ppq/4 = 480 ticks
    let grid_gap = ppq / 4.0;
    let mut current_tick = (start_tick / grid_gap).ceil() * grid_gap;

    // 创建不同级别的线条样式
    let bar_stroke = Stroke::default()
        .with_width(4.0)
        .with_color(theme.bar_line_color());
    let beat_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.beat_line_color());
    let sub_beat_stroke = Stroke::default()
        .with_width(0.5)
        .with_color(theme.half_beat_line_color());
    let grid_stroke = Stroke::default()
        .with_width(0.5)
        .with_color(theme.grid_line_color());

    while current_tick < end_tick {
        let screen_x = (current_tick * view.zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            let is_measure = (current_tick % measure_ticks).abs() < 0.1;
            let is_beat = (current_tick % ppq).abs() < 0.1;
            let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

            let stroke = if is_measure {
                bar_stroke
            } else if is_beat {
                beat_stroke
            } else if is_half_beat {
                sub_beat_stroke
            } else {
                grid_stroke
            };

            let path = Path::line(
                Point::new(screen_x, ruler_height),
                Point::new(screen_x, bounds.height),
            );
            frame.stroke(&path, stroke);
        }
        current_tick += grid_gap;
    }
}
