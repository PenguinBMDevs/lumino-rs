//! 力度/CC/Tempo Canvas 绘制函数
//!
//! 包含所有 Canvas 绘制逻辑：网格线、刻度标签、速度曲线、曲线绘制反馈等。

use iced_core::{Color, Point, Rectangle, Size, alignment, mouse};
use iced_widget::canvas::{self, Frame, path};

use crate::editor::editor_state::ViewState;
use crate::editor::grid::theme::ThemeExt;
use crate::{Renderer, Theme};

use super::super::{
    EditMode, VelocityPoint,
    PANEL_PADDING_X, PANEL_PADDING_Y, POINT_RADIUS, RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT,
};
use super::{TempoPoint, VelocityCanvas, VelocityCanvasState};

// ── Theme-aware colors ──

/// 面板背景色
pub fn velocity_bg_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return crate::theme::hc::RULER_BG;
    }
    let palette = theme.extended_palette().background;
    if theme.is_light() { palette.weakest.color } else { palette.base.color }
}

/// 面板网格线颜色
pub fn velocity_grid_line_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return crate::theme::hc::GRID_LINE;
    }
    let c = theme.extended_palette().background.strongest.color;
    let alpha = if theme.is_light() { 0.10 } else { 0.08 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 面板刻度标签颜色
pub fn velocity_text_color(theme: &Theme) -> Color {
    let c = theme.text_color();
    Color::from_rgba(c.r, c.g, c.b, 0.3)
}

/// 面板顶部边框线颜色
pub fn velocity_border_color(theme: &Theme) -> Color {
    let c = theme.border_color();
    let alpha = if theme.is_light() { 0.15 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// resize 手柄背景色
pub fn velocity_handle_bg_color(theme: &Theme, hovered: bool) -> Color {
    if crate::theme::is_high_contrast() {
        return if hovered {
            Color::from_rgba(1.0, 0.8, 0.0, 0.5)
        } else {
            Color::from_rgba(0.2, 0.2, 0.2, 0.3)
        };
    }
    let c = theme.extended_palette().background.strong.color;
    let alpha = if hovered { 0.5 } else { 0.25 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// resize grab 条颜色
pub fn velocity_grab_bar_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.5);
    }
    let c = theme.text_color();
    let alpha = if theme.is_light() { 0.40 } else { 0.35 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 曲线绘制影响范围底色
pub fn velocity_curve_range_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.12);
    }
    let c = theme.extended_palette().primary.base.color;
    let alpha = if theme.is_light() { 0.08 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 曲线绘制轨迹线颜色
pub fn velocity_curve_trail_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.6);
    }
    let c = theme.extended_palette().primary.base.color;
    Color::from_rgba(c.r, c.g, c.b, 0.5)
}

// ── Tempo 常量 ──

const TEMPO_BPM_MIN: f64 = 20.0;
const TEMPO_BPM_MAX: f64 = 10000.0;

/// 将 BPM 值映射到面板 Y 坐标
pub fn tempo_bpm_to_y(bpm: f64, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = ((bpm - TEMPO_BPM_MIN) / (TEMPO_BPM_MAX - TEMPO_BPM_MIN)) as f32;
    max_y - normalized * (max_y - min_y)
}

/// 生成 BPM 标尺刻度值
pub fn generate_tempo_levels() -> Vec<f64> {
    vec![TEMPO_BPM_MIN, 60.0, 120.0, 240.0, 480.0, 1000.0, 2000.0, 5000.0, TEMPO_BPM_MAX]
}

/// 将弯音值 (-8192 ~ +8191) 映射到面板 Y 坐标
pub fn bend_value_to_y(value: i16, bounds_height: f32) -> f32 {
    let max_y = bounds_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = (value as f32 + 8192.0) / 16383.0;
    max_y - normalized * (max_y - min_y)
}

/// 计算 Tempo 控制点屏幕位置
pub fn tempo_point_screen_pos(
    point: &TempoPoint,
    _bounds_width: f32,
    bounds_height: f32,
    view: &ViewState,
    min_bpm: f64,
    bpm_range: f64,
) -> Point {
    let x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
    let max_y = bounds_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = ((point.bpm - min_bpm) / bpm_range) as f32;
    let y = max_y - normalized * (max_y - min_y);
    Point::new(x, y)
}

// ── Drawing functions ──

/// 绘制面板背景（网格线 + 力度刻度）
pub fn draw_background(frame: &mut Frame<Renderer>, theme: &Theme, size: Size) {
    let width = size.width;
    let height = size.height;
    let draw_top = RESIZE_HANDLE_HEIGHT;
    let line_color = velocity_grid_line_color(theme);
    let text_color = velocity_text_color(theme);
    let velocity_levels = [0u8, 32, 64, 96, 127];

    for &v in &velocity_levels {
        let y = VelocityCanvas::velocity_to_y(v, height);
        let mut line_builder = path::Builder::new();
        line_builder.move_to(Point::new(PANEL_PADDING_X, y));
        line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
        frame.stroke(&line_builder.build(), canvas::Stroke::default().with_color(line_color).with_width(1.0));

        frame.fill_text(canvas::Text {
            content: format!("{}", v),
            position: Point::new(4.0, y - 6.0),
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

    let border_color = velocity_border_color(theme);
    frame.fill_rectangle(Point::new(0.0, draw_top), Size::new(width, 1.0), border_color);
}

/// 绘制顶部 resize 拖拽手柄
pub fn draw_resize_handle(frame: &mut Frame<Renderer>, theme: &Theme, size: Size, hovered: bool) {
    let handle_color = velocity_handle_bg_color(theme, hovered);
    let grab_bar_color = velocity_grab_bar_color(theme);

    frame.fill_rectangle(Point::new(0.0, 0.0), Size::new(size.width, RESIZE_HANDLE_HEIGHT), handle_color);

    let bar_width = 40.0;
    let bar_height = 3.0;
    let bar_x = (size.width - bar_width) / 2.0;
    let bar_y = (RESIZE_HANDLE_HEIGHT - bar_height) / 2.0;
    frame.fill_rectangle(Point::new(bar_x, bar_y), Size::new(bar_width, bar_height), grab_bar_color);
}

/// 绘制速度（Tempo）折线图
pub fn draw_tempo_graph(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    points: &[TempoPoint],
    size: Size,
    view: &ViewState,
) {
    if points.is_empty() { return; }

    let width = size.width;
    let height = size.height;
    let line_color = theme.extended_palette().secondary.strong.color;
    let point_color = theme.extended_palette().secondary.base.color;
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
    if screen_points.is_empty() { return; }

    screen_points.sort_by(|a, b| a.0.x.partial_cmp(&b.0.x).unwrap_or(std::cmp::Ordering::Equal));

    // 绘制折线
    let mut line_builder = path::Builder::new();
    line_builder.move_to(screen_points[0].0);
    for &(pos, _) in screen_points.iter().skip(1) { line_builder.line_to(pos); }
    frame.stroke(&line_builder.build(), canvas::Stroke::default().with_color(line_color).with_width(2.0));

    // 合批绘制控制点
    let mut circle_builder = path::Builder::new();
    for &(pos, _) in &screen_points { circle_builder.circle(pos, POINT_RADIUS); }
    frame.fill(&circle_builder.build(), point_color);

    // BPM 标签
    for &(pos, bpm) in &screen_points {
        frame.fill_text(canvas::Text {
            content: format!("{:.0}", bpm),
            position: Point::new(pos.x - 10.0, pos.y - 14.0),
            max_width: width,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(9.0),
            color: Color::from_rgba(0.6, 0.6, 0.6, 0.7),
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
    size: Size,
    view: &ViewState,
    cursor: mouse::Cursor,
    bounds: Rectangle,
) {
    let width = size.width;
    let height = size.height;
    let start_x = state.curve_start_x;
    let cursor_local = cursor.position().map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
    let Some(current_pos) = cursor_local else { return; };
    let current_x = current_pos.x;
    let min_x = start_x.min(current_x);
    let max_x = start_x.max(current_x);

    let range_color = velocity_curve_range_color(theme);
    frame.fill_rectangle(Point::new(min_x, 0.0), Size::new(max_x - min_x, height), range_color);

    let start_vel = state.curve_start_velocity;
    let current_vel = VelocityCanvas::y_to_velocity(current_pos.y, height);
    let start_y = VelocityCanvas::velocity_to_y(start_vel, height);
    let current_y = VelocityCanvas::velocity_to_y(current_vel, height);

    let trail_color = velocity_curve_trail_color(theme);
    let mut trail_builder = path::Builder::new();
    trail_builder.move_to(Point::new(start_x, start_y));
    trail_builder.line_to(Point::new(current_x, current_y));
    frame.stroke(&trail_builder.build(), canvas::Stroke::default().with_color(trail_color).with_width(2.0));

    let affected_color = theme.extended_palette().secondary.strong.color;
    for point in points {
        if !state.curve_affected.contains_key(&point.note_index) { continue; }
        let pos = VelocityCanvas::point_screen_pos(point, 0, width, height, view);
        let glow = Color::from_rgba(affected_color.r, affected_color.g, affected_color.b, 0.4);
        frame.fill(&canvas::Path::circle(pos, POINT_RADIUS + 4.0), glow);
        frame.fill(&canvas::Path::circle(pos, POINT_RADIUS + 1.0), affected_color);
    }
}

/// 绘制竖向网格线（小节线/拍线/半拍线）
pub fn draw_vertical_lines(frame: &mut Frame<Renderer>, theme: &Theme, size: Size, view: &ViewState) {
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
        let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if x >= view.keyboard_width && x <= width {
            frame.fill_rectangle(Point::new(x, line_y), Size::new(1.0, line_h), Color { a: 0.5, ..bar_color });
        }
    }

    let beat_color = theme.beat_line_color();
    let beat_start = (visible_tick_start / ticks_per_beat).floor() as u32;
    let beat_end = (visible_tick_end / ticks_per_beat).ceil() as u32;
    for beat in beat_start..=beat_end {
        let tick = beat as f32 * ticks_per_beat;
        if (tick % ticks_per_measure as f32).abs() < f32::EPSILON { continue; }
        let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if x >= view.keyboard_width && x <= width {
            frame.fill_rectangle(Point::new(x, line_y), Size::new(1.0, line_h), Color { a: 0.3, ..beat_color });
        }
    }

    if view.zoom_x > 0.05 {
        let half_beat_color = theme.half_beat_line_color();
        let ticks_per_half_beat = ppq / 2.0;
        let half_beat_start = (visible_tick_start / ticks_per_half_beat).floor() as u32;
        let half_beat_end = (visible_tick_end / ticks_per_half_beat).ceil() as u32;
        for hb in half_beat_start..=half_beat_end {
            let tick = hb as f32 * ticks_per_half_beat;
            if (tick % ticks_per_measure as f32).abs() < f32::EPSILON
                || (tick % ticks_per_beat).abs() < f32::EPSILON { continue; }
            let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if x >= view.keyboard_width && x <= width {
                frame.fill_rectangle(Point::new(x, line_y), Size::new(1.0, line_h), Color { a: 0.15, ..half_beat_color });
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
            for &v in &scale_values {
                let y = VelocityCanvas::velocity_to_y(v, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                frame.stroke(&line_builder.build(), canvas::Stroke::default().with_color(line_color).with_width(1.0));
            }
        }
        EditMode::Bend => {
            let bend_values: [i16; 5] = [-8192, -4096, 0, 4096, 8191];
            for &v in &bend_values {
                let y = bend_value_to_y(v, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                frame.stroke(&line_builder.build(), canvas::Stroke::default().with_color(line_color).with_width(1.0));
            }
        }
        EditMode::Tempo => {
            let bpm_levels = generate_tempo_levels();
            for &bpm in &bpm_levels {
                let y = tempo_bpm_to_y(bpm, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                frame.stroke(&line_builder.build(), canvas::Stroke::default().with_color(line_color).with_width(1.0));
            }
        }
    }
}

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
            for &v in &scale_values {
                let y = VelocityCanvas::velocity_to_y(v, size.height);
                frame.fill_text(canvas::Text {
                    content: format!("{}", v),
                    position: Point::new(4.0, y - 6.0),
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
            let bend_labels: [(i16, &str); 5] = [(-8192, "-8k"), (-4096, "-4k"), (0, "0"), (4096, "+4k"), (8191, "+8k")];
            for &(v, label) in &bend_labels {
                let y = bend_value_to_y(v, size.height);
                frame.fill_text(canvas::Text {
                    content: label.to_string(),
                    position: Point::new(4.0, y - 6.0),
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
                let y = tempo_bpm_to_y(bpm, size.height);
                frame.fill_text(canvas::Text {
                    content: format!("{:.0}", bpm),
                    position: Point::new(4.0, y - 6.0),
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
