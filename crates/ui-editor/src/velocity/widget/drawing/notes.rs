//! 音符/曲线绘制函数
use super::*;

/// 绘制速度（Tempo）折线图
pub fn draw_tempo_graph(
    frame: &mut Frame<Renderer>,
    points: &[TempoPoint],
    size: Size,
    view: &ViewState,
    line_thickness: f32,
) {
    if points.is_empty() {
        return;
    }

    let width = size.width;
    let height = size.height;
    // 自动化节点统一蓝色，与主音轨音符视觉一致
    let node_color = automation_node_color();
    let line_color = node_color;
    let point_color = node_color;
    let min_bpm = TEMPO_BPM_MIN;
    let bpm_range = TEMPO_BPM_MAX - TEMPO_BPM_MIN;

    let mut screen_points: Vec<(Point, f64)> = Vec::new();
    for p in points {
        let x = p.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if x >= -50.0 && x <= width + 50.0 {
            let pos = tempo_point_screen_pos(p, width, height, view, min_bpm, bpm_range);
            screen_points.push((pos, p.bpm));
        }
    }
    if screen_points.is_empty() {
        return;
    }

    screen_points.sort_by(|a, b| {
        a.0.x
            .partial_cmp(&b.0.x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 绘制折线
    let mut line_builder = path::Builder::new();
    line_builder.move_to(screen_points[0].0);
    for &(pos, _) in screen_points.iter().skip(1) {
        line_builder.line_to(pos);
    }
    frame.stroke(
        &line_builder.build(),
        canvas::Stroke::default()
            .with_color(line_color)
            .with_width(line_thickness),
    );

    // 合批绘制控制点
    let mut circle_builder = path::Builder::new();
    for &(pos, _) in &screen_points {
        circle_builder.circle(pos, POINT_RADIUS);
    }
    frame.fill(&circle_builder.build(), point_color);

    // BPM 标签
    for &(pos, bpm) in &screen_points {
        frame.fill_text(canvas::Text {
            content: format!("{:.0}", bpm),
            position: Point::new(pos.x - 10.0, pos.y - 14.0),
            max_width: width,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(9.0),
            color: Color::from_rgba(0.2, 0.55, 1.0, 0.7),
            font: iced_core::Font::DEFAULT,
            align_x: alignment::Horizontal::Center.into(),
            align_y: alignment::Vertical::Top,
            shaping: iced_core::text::Shaping::Basic,
        });
    }
}

/// 绘制曲线绘制模式的视觉反馈
pub fn draw_curve_paint_feedback(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    points: &[VelocityPoint],
    state: &VelocityCanvasState,
    canvas_params: &CurvePaintCanvasParams,
    cursor: mouse::Cursor,
    line_thickness: f32,
) {
    let width = canvas_params.size.width;
    let height = canvas_params.size.height;
    let start_x = state.curve_start_x;
    let cursor_local = cursor
        .position()
        .map(|p| Point::new(p.x - canvas_params.bounds.x, p.y - canvas_params.bounds.y));
    let Some(current_pos) = cursor_local else {
        return;
    };
    let current_x = current_pos.x;
    let min_x = start_x.min(current_x);
    let max_x = start_x.max(current_x);

    let range_color = velocity_curve_range_color(theme);
    frame.fill_rectangle(
        Point::new(min_x, 0.0),
        Size::new(max_x - min_x, height),
        range_color,
    );

    let start_vel = state.curve_start_velocity;
    let current_vel = VelocityCanvas::y_to_velocity(current_pos.y, height);
    let start_y = VelocityCanvas::velocity_to_y(start_vel, height);
    let current_y = VelocityCanvas::velocity_to_y(current_vel, height);

    // 自动化曲线反馈统一蓝色，与主音轨音符视觉一致
    let trail_color = automation_node_color();
    let mut trail_builder = path::Builder::new();
    trail_builder.move_to(Point::new(start_x, start_y));
    trail_builder.line_to(Point::new(current_x, current_y));
    frame.stroke(
        &trail_builder.build(),
        canvas::Stroke::default()
            .with_color(trail_color)
            .with_width(line_thickness),
    );

    let affected_color = automation_node_color();
    for point in points {
        if !state.curve_affected.contains_key(&point.note_index) {
            continue;
        }
        let pos = VelocityCanvas::point_screen_pos(point, 0, width, height, &canvas_params.view);
        let glow = Color::from_rgba(affected_color.r, affected_color.g, affected_color.b, 0.4);
        frame.fill(&canvas::Path::circle(pos, POINT_RADIUS + 4.0), glow);
        frame.fill(
            &canvas::Path::circle(pos, POINT_RADIUS + 1.0),
            affected_color,
        );
    }
}
