//! 时间轴标尺绘制

use super::theme::ThemeExt;
use crate::Renderer;
use crate::constants::editor::MEASURE_NUMBER_FONT_SIZE;
use crate::editor::Editor;
use iced_core::{Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, Geometry, Path, Stroke, Text};

/// 绘制时间轴标尺到 Geometry（用于 Canvas 绘制）
pub fn draw_to_geometry(
    editor: &Editor,
    renderer: &Renderer,
    bounds: Rectangle,
    theme: &crate::Theme,
) -> Geometry<Renderer> {
    let mut frame = Frame::new(renderer, bounds.size());
    draw(editor, &mut frame, bounds, theme);
    frame.into_geometry()
}

/// 绘制时间轴标尺（小节号显示区域）
pub fn draw(editor: &Editor, frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &crate::Theme) {
    let view = &editor.state;
    let ppq = view.ppq as f32;
    let keyboard_width = view.keyboard_width;
    let ruler_height = view.ruler_height;

    let measure_ticks = ppq * 4.0;
    let start_tick = view.scroll_x / view.zoom_x;
    let end_tick = (view.scroll_x + bounds.width - keyboard_width) / view.zoom_x;

    // 绘制标尺背景
    let ruler_bg_color = theme.ruler_background_color();
    let ruler_rect = Rectangle::new(
        Point::new(keyboard_width, 0.0),
        Size::new(bounds.width - keyboard_width, ruler_height),
    );
    let ruler_path = Path::rectangle(ruler_rect.position(), ruler_rect.size());
    frame.fill(&ruler_path, ruler_bg_color);

    // 绘制标尺边框
    let border_stroke = Stroke::default()
        .with_width(1.0)
        .with_color(theme.border_color());
    frame.stroke(&ruler_path, border_stroke);

    let text_color = theme.text_color();

    // 绘制小节号和刻度线
    let mut current_measure_tick = ((start_tick / measure_ticks).floor() * measure_ticks).max(0.0);
    let mut measure_number = (current_measure_tick / measure_ticks).ceil() as u32;

    while current_measure_tick <= end_tick {
        let screen_x = (current_measure_tick * view.zoom_x) - view.scroll_x + keyboard_width;

        if screen_x >= keyboard_width && screen_x <= bounds.width {
            // 绘制小节号文本
            let measure_text = Text {
                content: measure_number.to_string(),
                position: Point::new(screen_x + 4.0, 4.0),
                max_width: bounds.width - keyboard_width,
                line_height: iced_core::text::LineHeight::Relative(1.0),
                size: iced_core::Pixels(MEASURE_NUMBER_FONT_SIZE),
                color: text_color,
                font: iced_core::Font::DEFAULT,
                align_x: alignment::Horizontal::Left.into(),
                align_y: alignment::Vertical::Top,
                shaping: iced_core::text::Shaping::Basic,
            };
            frame.fill_text(measure_text);

            // 绘制刻度线
            let tick_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(theme.border_color());
            let tick_path = Path::line(
                Point::new(screen_x, 0.0),
                Point::new(screen_x, ruler_height),
            );
            frame.stroke(&tick_path, tick_stroke);
        }

        current_measure_tick += measure_ticks;
        measure_number += 1;
    }
}
