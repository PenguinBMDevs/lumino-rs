//! 小节线/网格线绘制

use super::theme::ThemeExt;
use crate::Renderer;
use crate::editor::Editor;
use iced_core::{Point, Rectangle};
use iced_widget::canvas::path::Builder;
use iced_widget::canvas::{Frame, Stroke};

/// 绘制小节线和拍线（纵向线）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let view = &editor.editor_state.view;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let measure_ticks = ppq * 4.0;
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

    // 自适应网格线间隔：根据缩放级别自动调整密度
    let grid_gap = super::utils::adaptive_grid_gap(view.zoom_x, ppq);

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

    // 使用 path builder 批量绘制同级别的线条
    let mut bar_builder = Builder::new();
    let mut beat_builder = Builder::new();
    let mut sub_beat_builder = Builder::new();
    let mut grid_builder = Builder::new();

    while current_tick < end_tick {
        let screen_x = (current_tick * view.zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            let is_measure = (current_tick % measure_ticks).abs() < 0.1;
            let is_beat = (current_tick % ppq).abs() < 0.1;
            let is_half_beat = (current_tick % (ppq / 2.0)).abs() < 0.1;

            let builder = if is_measure {
                &mut bar_builder
            } else if is_beat {
                &mut beat_builder
            } else if is_half_beat {
                &mut sub_beat_builder
            } else {
                &mut grid_builder
            };

            builder.move_to(Point::new(screen_x, ruler_height));
            builder.line_to(Point::new(screen_x, bounds.height));
        }
        current_tick += grid_gap;
    }

    // 批量绘制各级别线条
    let bar_path = bar_builder.build();
    let beat_path = beat_builder.build();
    let sub_beat_path = sub_beat_builder.build();
    let grid_path = grid_builder.build();

    frame.stroke(&bar_path, bar_stroke);
    frame.stroke(&beat_path, beat_stroke);
    frame.stroke(&sub_beat_path, sub_beat_stroke);
    frame.stroke(&grid_path, grid_stroke);
}
