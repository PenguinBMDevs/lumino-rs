//! 标签绘制函数
use super::*;

/// 绘制刻度标签文字
pub fn draw_scale_labels(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    size: Size,
    edit_mode: EditMode,
) {
    let text_color = velocity_text_color(theme);
    let width = size.width;

    match edit_mode {
        EditMode::Velocity | EditMode::Cc(_) => {
            let scale_values = [0u8, 32, 64, 96, 127];
            for &value in &scale_values {
                let label_y = VelocityCanvas::velocity_to_y(value, size.height);
                frame.fill_text(canvas::Text {
                    content: format!("{}", value),
                    position: Point::new(4.0, label_y - 6.0),
                    max_width: width,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(9.0),
                    color: text_color,
                    font: iced_core::Font::DEFAULT,
                    align_x: alignment::Horizontal::Left.into(),
                    align_y: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                });
            }
        }
        EditMode::Bend => {
            let bend_labels: [(i16, &str); 5] = [
                (-8192, "-8k"),
                (-4096, "-4k"),
                (0, "0"),
                (4096, "+4k"),
                (8191, "+8k"),
            ];
            for &(value, label) in &bend_labels {
                let label_y = bend_value_to_y(value, size.height);
                frame.fill_text(canvas::Text {
                    content: label.to_string(),
                    position: Point::new(4.0, label_y - 6.0),
                    max_width: width,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(9.0),
                    color: text_color,
                    font: iced_core::Font::DEFAULT,
                    align_x: alignment::Horizontal::Left.into(),
                    align_y: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                });
            }
        }
        EditMode::Tempo => {
            let bpm_levels = generate_tempo_levels();
            for &bpm in &bpm_levels {
                let label_y = tempo_bpm_to_y(bpm, size.height);
                frame.fill_text(canvas::Text {
                    content: format!("{:.0}", bpm),
                    position: Point::new(4.0, label_y - 6.0),
                    max_width: width,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(9.0),
                    color: text_color,
                    font: iced_core::Font::DEFAULT,
                    align_x: alignment::Horizontal::Left.into(),
                    align_y: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                });
            }
        }
    }
}
