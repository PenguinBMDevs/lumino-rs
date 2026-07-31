//! 网格绘制函数
use super::*;

/// 绘制竖向网格线（小节线/拍线/半拍线）
pub fn draw_vertical_lines(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    size: Size,
    view: &ViewState,
) {
    let height = size.height;
    let width = size.width;
    let ppq = view.ppq as f32;
    let ticks_per_beat = ppq;
    let ticks_per_measure = ppq * 4.0;
    let visible_tick_start = view.scroll_x / view.zoom_x;
    let visible_tick_end = (view.scroll_x + width - view.keyboard_width) / view.zoom_x;
    let line_y = RESIZE_HANDLE_HEIGHT;
    let line_h = height - 2.0 * RESIZE_HANDLE_HEIGHT;

    let bar_color = theme.bar_line_color();
    let measure_start = (visible_tick_start / ticks_per_measure).floor() as u32;
    let measure_end = (visible_tick_end / ticks_per_measure).ceil() as u32;
    for measure in measure_start..=measure_end {
        let tick = measure as f32 * ticks_per_measure;
        let grid_x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if grid_x >= view.keyboard_width && grid_x <= width {
            frame.fill_rectangle(
                Point::new(grid_x, line_y),
                Size::new(1.0, line_h),
                Color {
                    a: 0.5,
                    ..bar_color
                },
            );
        }
    }

    let beat_color = theme.beat_line_color();
    let beat_start = (visible_tick_start / ticks_per_beat).floor() as u32;
    let beat_end = (visible_tick_end / ticks_per_beat).ceil() as u32;
    for beat in beat_start..=beat_end {
        let tick = beat as f32 * ticks_per_beat;
        if (tick % ticks_per_measure).abs() < f32::EPSILON {
            continue;
        }
        let grid_x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if grid_x >= view.keyboard_width && grid_x <= width {
            frame.fill_rectangle(
                Point::new(grid_x, line_y),
                Size::new(1.0, line_h),
                Color {
                    a: 0.3,
                    ..beat_color
                },
            );
        }
    }

    if view.zoom_x > 0.05 {
        let half_beat_color = theme.half_beat_line_color();
        let ticks_per_half_beat = ppq / 2.0;
        let half_beat_start = (visible_tick_start / ticks_per_half_beat).floor() as u32;
        let half_beat_end = (visible_tick_end / ticks_per_half_beat).ceil() as u32;
        for hb in half_beat_start..=half_beat_end {
            let tick = hb as f32 * ticks_per_half_beat;
            if (tick % ticks_per_measure).abs() < f32::EPSILON
                || (tick % ticks_per_beat).abs() < f32::EPSILON
            {
                continue;
            }
            let grid_x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if grid_x >= view.keyboard_width && grid_x <= width {
                frame.fill_rectangle(
                    Point::new(grid_x, line_y),
                    Size::new(1.0, line_h),
                    Color {
                        a: 0.15,
                        ..half_beat_color
                    },
                );
            }
        }
    }
}

/// 绘制横向参考线（Velocity/CC/Bend/Tempo 模式）
pub fn draw_horizontal_lines(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    size: Size,
    edit_mode: EditMode,
) {
    let width = size.width;
    let line_color = velocity_grid_line_color(theme);

    match edit_mode {
        EditMode::Velocity | EditMode::Cc(_) => {
            let scale_values = [0u8, 32, 64, 96, 127];
            for &value in &scale_values {
                let velocity_y = VelocityCanvas::velocity_to_y(value, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, velocity_y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, velocity_y));
                frame.stroke(
                    &line_builder.build(),
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
        EditMode::Bend => {
            let bend_values: [i16; 5] = [-8192, -4096, 0, 4096, 8191];
            for &value in &bend_values {
                let velocity_y = bend_value_to_y(value, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, velocity_y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, velocity_y));
                frame.stroke(
                    &line_builder.build(),
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
        EditMode::Tempo => {
            let bpm_levels = generate_tempo_levels();
            for &bpm in &bpm_levels {
                let velocity_y = tempo_bpm_to_y(bpm, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, velocity_y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, velocity_y));
                frame.stroke(
                    &line_builder.build(),
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
    }
}
