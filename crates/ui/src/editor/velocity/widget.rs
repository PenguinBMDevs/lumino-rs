//! 力度面板 Canvas 绘制与交互
//!
//! 类 Cubase 的力度描点编辑器：
//! - 描点 + 连线（力度包络线）
//! - 描点只能垂直拖动（上下调整力度值）
//! - 悬停高亮 + 拖拽实时反馈
//! - 顶部拖拽手柄可调整面板高度
//!
//! 也支持速度（Tempo）包络绘制：Conductor 音轨上显示全局速度曲线。

use std::collections::HashMap;

use iced_core::{Color, Point, Rectangle, Size, alignment, mouse};
use iced_widget::canvas::{self, Frame, Program, path};

use crate::editor::editor_state::ViewState;
use crate::editor::grid::theme::ThemeExt;
use crate::message::VelocityAction;
use crate::{Message, Renderer, Theme};

// iced_wgpu::Geometry is the concrete canvas geometry type for the wgpu backend
use iced_wgpu::Geometry as Geom;

use super::*;

/// 速度控制点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// tick 位置
    pub tick: f32,
    /// BPM 值 (20-10000)
    pub bpm: f64,
}

/// 力度 Canvas 状态
#[derive(Debug, Default)]
pub struct VelocityCanvasState {
    /// 当前正在拖拽的点索引（在 points 中的索引，非 note_index）
    pub drag_point_idx: Option<usize>,
    /// 拖拽开始时的力度值（用于 undo）
    pub _drag_start_velocity: u8,
    /// 当前悬停的点索引
    pub hover_point_idx: Option<usize>,
    /// Canvas 是否已初始化尺寸
    pub _initialized: bool,
    /// 是否在拖拽 resize 手柄
    pub resize_dragging: bool,
    /// resize 拖拽起始 Y 坐标（绝对屏幕坐标）
    pub resize_drag_start_y: f32,
    /// resize 拖拽开始时的面板高度
    pub resize_start_height: f32,
    /// 鼠标是否悬停在 resize 手柄区域
    pub hover_resize_handle: bool,
    /// 是否正在曲线绘制模式
    pub curve_active: bool,
    /// 曲线绘制起始 X 坐标（local）
    pub curve_start_x: f32,
    /// 曲线绘制起始 Y 对应的力度值
    pub curve_start_velocity: u8,
    /// 当前笔触影响的音符索引 → 新力度值
    pub curve_affected: HashMap<usize, u8>,
}

/// 力度/CC Canvas 程序
pub struct VelocityCanvas<'a> {
    pub editor: &'a crate::editor::Editor,
    /// 当前编辑模式
    pub edit_mode: super::EditMode,
    /// CC 模式下选择的控制器编号
    pub selected_cc: u8,
}

impl<'a> VelocityCanvas<'a> {
    /// 获取所有力度点
    fn points(&self) -> Vec<VelocityPoint> {
        let notes = &self.editor.editor_state.data.notes;
        VelocityPanel::build_velocity_points(notes)
    }

    /// 将力度值映射到 Y 坐标（panel 底部 = 0 velocity, 顶部 = 127 velocity）
    fn velocity_to_y(velocity: u8, bounds_height: f32) -> f32 {
        let draw_height = bounds_height - RESIZE_HANDLE_HEIGHT;
        let max_y = draw_height - PANEL_PADDING_Y;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let normalized = velocity as f32 / 127.0;
        max_y - normalized * (max_y - min_y)
    }

    /// 将 Y 坐标映射回力度值 (0-127)
    fn y_to_velocity(y: f32, bounds_height: f32) -> u8 {
        let draw_height = bounds_height - RESIZE_HANDLE_HEIGHT;
        let max_y = draw_height - PANEL_PADDING_Y;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let clamped_y = y.clamp(min_y, max_y);
        let normalized = (max_y - clamped_y) / (max_y - min_y);
        (normalized * 127.0).round().clamp(0.0, 127.0) as u8
    }

    /// 获取点的屏幕位置（X 坐标与对应音符头部对齐）
    fn point_screen_pos(
        point: &VelocityPoint,
        _index: usize,
        _bounds_width: f32,
        bounds_height: f32,
        view: &ViewState,
    ) -> Point {
        let x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        let y = Self::velocity_to_y(point.velocity, bounds_height);
        Point::new(x, y)
    }

    /// 命中测试：寻找点击位置最近的力度点
    fn hit_test(
        points: &[VelocityPoint],
        click_pos: Point,
        bounds_width: f32,
        bounds_height: f32,
        view: &ViewState,
    ) -> Option<usize> {
        let mut closest: Option<(usize, f32)> = None;
        for (i, point) in points.iter().enumerate() {
            let pos = Self::point_screen_pos(point, i, bounds_width, bounds_height, view);
            let dx = click_pos.x - pos.x;
            let dy = click_pos.y - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < HIT_RADIUS {
                match closest {
                    None => closest = Some((i, dist)),
                    Some((_, best_dist)) if dist < best_dist => closest = Some((i, dist)),
                    _ => {}
                }
            }
        }
        closest.map(|(idx, _)| idx)
    }

    /// 判断光标是否在 resize 手柄区域
    fn is_in_resize_zone(cursor_pos: Point) -> bool {
        (0.0..=RESIZE_HANDLE_HEIGHT).contains(&cursor_pos.y)
    }

    /// 更新曲线绘制：计算当前鼠标位置影响的力度点
    fn update_curve_paint(
        state: &mut VelocityCanvasState,
        points: &[VelocityPoint],
        cursor_pos: Point,
        bounds_size: Size,
        view: &ViewState,
        selected_notes: &std::collections::HashSet<usize>,
    ) -> Option<canvas::Action<Message>> {
        let start_x = state.curve_start_x;
        let current_x = cursor_pos.x;
        let min_x = start_x.min(current_x);
        let max_x = start_x.max(current_x);
        let current_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
        let start_velocity = state.curve_start_velocity;

        // 有选中音符时，只影响选中的
        let has_selection = !selected_notes.is_empty();

        let mut updates: Vec<(usize, u8)> = Vec::new();

        for point in points {
            let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;

            // 跳过水平范围外的点
            if point_x < min_x || point_x > max_x {
                continue;
            }

            // 如有选中音符，只影响选中的
            if has_selection && !selected_notes.contains(&point.note_index) {
                continue;
            }

            // 计算插值力度：基于点 X 在起始和当前 X 之间的位置
            let t = if (max_x - min_x).abs() < f32::EPSILON {
                1.0
            } else {
                (point_x - min_x) / (max_x - min_x)
            };
            let interp_velocity_f = start_velocity as f32 * (1.0 - t) + current_velocity as f32 * t;
            let new_velocity = interp_velocity_f.round().clamp(0.0, 127.0) as u8;

            if point.velocity != new_velocity {
                state.curve_affected.insert(point.note_index, new_velocity);
                updates.push((point.note_index, new_velocity));
            }
        }

        if updates.is_empty() {
            return None;
        }

        Some(canvas::Action::publish(Message::Velocity(
            VelocityAction::CurvePaint(updates),
        )))
    }
}

impl Program<Message, Theme, Renderer> for VelocityCanvas<'_> {
    type State = VelocityCanvasState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let bounds_size = bounds.size();

        if !state._initialized {
            state._initialized = true;
        }

        if bounds_size.width <= PANEL_PADDING_X * 2.0 {
            return None;
        }

        let cursor_pos = match cursor.position() {
            Some(pos) => Point::new(pos.x - bounds.x, pos.y - bounds.y),
            None => return None,
        };

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                // 检查是否在 resize 手柄区域
                if Self::is_in_resize_zone(cursor_pos) {
                    state.resize_dragging = true;
                    state.resize_drag_start_y = cursor.position().unwrap_or_default().y;
                    state.resize_start_height = bounds_size.height;
                    return None;
                }

                let points = self.points();
                let view = &self.editor.editor_state.view;
                if points.is_empty() {
                    return None;
                }

                if let Some(point_idx) = Self::hit_test(
                    &points,
                    cursor_pos,
                    bounds_size.width,
                    bounds_size.height,
                    view,
                ) {
                    state.drag_point_idx = Some(point_idx);
                    state._drag_start_velocity = points[point_idx].velocity;
                    return Some(canvas::Action::publish(Message::Velocity(
                        VelocityAction::DragStart(
                            points[point_idx].note_index,
                            points[point_idx].velocity,
                        ),
                    )));
                }

                // 点击空白区域 → 进入曲线绘制模式（仅限有效绘制区域内）
                let in_draw_area = cursor_pos.x >= 0.0
                    && cursor_pos.x <= bounds_size.width
                    && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
                    && cursor_pos.y <= bounds_size.height;
                if !in_draw_area {
                    return None;
                }
                state.curve_active = true;
                state.curve_start_x = cursor_pos.x;
                state.curve_start_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
                state.curve_affected.clear();
                state.drag_point_idx = None;
                state.hover_point_idx = None;
                Some(canvas::Action::publish(Message::Velocity(
                    VelocityAction::CurveStart,
                )))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // 检查 resize 拖拽
                if state.resize_dragging {
                    let abs_cursor_y = cursor.position().unwrap_or_default().y;
                    let delta_y = state.resize_drag_start_y - abs_cursor_y;
                    let new_height = (state.resize_start_height + delta_y)
                        .clamp(VELOCITY_PANEL_MIN_HEIGHT, VELOCITY_PANEL_MAX_HEIGHT);
                    if (new_height - bounds_size.height).abs() > 1.0 {
                        return Some(canvas::Action::publish(Message::VelocityPanelResize(
                            new_height,
                        )));
                    }
                    return None;
                }

                // 更新 resize 手柄悬停状态
                state.hover_resize_handle = Self::is_in_resize_zone(cursor_pos);

                // 曲线绘制模式
                if state.curve_active {
                    // 鼠标移出面板边界时自动结束曲线绘制
                    let out_of_bounds = cursor_pos.x < 0.0
                        || cursor_pos.x > bounds_size.width
                        || cursor_pos.y < RESIZE_HANDLE_HEIGHT
                        || cursor_pos.y > bounds_size.height;
                    if out_of_bounds {
                        state.curve_active = false;
                        state.curve_affected.clear();
                        return Some(canvas::Action::publish(Message::Velocity(
                            VelocityAction::CurveEnd,
                        )));
                    }
                    let points = self.points();
                    if points.is_empty() {
                        return None;
                    }
                    let view = &self.editor.editor_state.view;
                    let selected_notes = &self.editor.editor_state.interaction.selected_notes;
                    return Self::update_curve_paint(
                        state,
                        &points,
                        cursor_pos,
                        bounds_size,
                        view,
                        selected_notes,
                    );
                }

                // 拖拽力度点
                let points = self.points();
                let view = &self.editor.editor_state.view;
                if points.is_empty() {
                    state.hover_point_idx = None;
                    return None;
                }

                if let Some(drag_idx) = state.drag_point_idx {
                    if drag_idx < points.len() {
                        let new_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
                        let old_velocity = points[drag_idx].velocity;
                        if new_velocity != old_velocity {
                            let note_index = points[drag_idx].note_index;
                            return Some(canvas::Action::publish(Message::Velocity(
                                VelocityAction::DragMove(note_index, new_velocity),
                            )));
                        }
                    }
                    return None;
                }

                let hover_idx = Self::hit_test(
                    &points,
                    cursor_pos,
                    bounds_size.width,
                    bounds_size.height,
                    view,
                );
                if hover_idx != state.hover_point_idx {
                    state.hover_point_idx = hover_idx;
                }
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.resize_dragging {
                    state.resize_dragging = false;
                    return None;
                }

                if state.curve_active {
                    state.curve_active = false;
                    state.curve_affected.clear();
                    return Some(canvas::Action::publish(Message::Velocity(
                        VelocityAction::CurveEnd,
                    )));
                }

                let was_dragging = state.drag_point_idx.is_some();
                state.drag_point_idx = None;
                state._drag_start_velocity = 0;
                if was_dragging {
                    return Some(canvas::Action::publish(Message::Velocity(
                        VelocityAction::DragEnd,
                    )));
                }
                None
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geom> {
        match self.edit_mode {
            super::EditMode::Tempo => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
                let tempo_points = VelocityPanel::build_tempo_points(self.editor);
                if !tempo_points.is_empty() {
                    draw_tempo_graph(&mut frame, theme, &tempo_points, bounds.size(), view);
                }
                return vec![frame.into_geometry()];
            }
            _ => {
                // 非 Tempo 模式：只画网格线和刻度标签
                // 背景、柱状条、resize 手柄由 wgpu 离屏渲染处理
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
                return vec![frame.into_geometry()];
            }
        }
    }

    fn mouse_interaction(
        &self,
        state: &Self::State,
        _bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if state.resize_dragging {
            return mouse::Interaction::ResizingVertically;
        }

        if let Some(cursor_pos) = cursor.position() {
            let local_y = cursor_pos.y - _bounds.y;
            if (0.0..=RESIZE_HANDLE_HEIGHT).contains(&local_y) {
                return mouse::Interaction::ResizingVertically;
            }
        }

        if state.curve_active {
            return mouse::Interaction::Crosshair;
        }

        if state.drag_point_idx.is_some() {
            mouse::Interaction::ResizingVertically
        } else if state.hover_point_idx.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

/// ── 面板主题色映射（theme-aware colors） ──
///
/// 这些辅助函数将 velocity 面板的颜色映射到 iced 主题的 extended palette，
/// 确保在 Dracula/Nord/Catppuccin/TokyoNight 等所有主题下颜色都正确。

/// 面板背景色：与标尺背景一致，使用 background 色系
fn velocity_bg_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return crate::theme::hc::RULER_BG;
    }
    let palette = theme.extended_palette().background;
    if theme.is_light() {
        palette.weakest.color
    } else {
        palette.base.color
    }
}

/// 面板网格线颜色：使用 background 对比色 + 低透明度
fn velocity_grid_line_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return crate::theme::hc::GRID_LINE;
    }
    let c = theme.extended_palette().background.strongest.color;
    let alpha = if theme.is_light() { 0.10 } else { 0.08 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 面板文字（力度刻度标签）颜色
fn velocity_text_color(theme: &Theme) -> Color {
    let c = theme.text_color();
    Color::from_rgba(c.r, c.g, c.b, 0.3)
}

/// 面板顶部边框线颜色
fn velocity_border_color(theme: &Theme) -> Color {
    let c = theme.border_color();
    let alpha = if theme.is_light() { 0.15 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// resize 手柄背景色
fn velocity_handle_bg_color(theme: &Theme, hovered: bool) -> Color {
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
fn velocity_grab_bar_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.5);
    }
    let c = theme.text_color();
    let alpha = if theme.is_light() { 0.40 } else { 0.35 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 曲线绘制影响范围底色：使用 primary 主题色
fn velocity_curve_range_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.12);
    }
    let c = theme.extended_palette().primary.base.color;
    let alpha = if theme.is_light() { 0.08 } else { 0.12 };
    Color::from_rgba(c.r, c.g, c.b, alpha)
}

/// 曲线绘制轨迹线颜色
fn velocity_curve_trail_color(theme: &Theme) -> Color {
    if crate::theme::is_high_contrast() {
        return Color::from_rgba(1.0, 0.8, 0.0, 0.6);
    }
    let c = theme.extended_palette().primary.base.color;
    Color::from_rgba(c.r, c.g, c.b, 0.5)
}

/// 绘制面板背景（网格线 + 力度刻度）
/// 注意：背景填充由 wgpu 离屏 clear_color 提供，Canvas 只画网格线和文字
fn draw_background(frame: &mut Frame<Renderer>, theme: &Theme, size: Size) {
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
        let line_path = line_builder.build();
        frame.stroke(
            &line_path,
            canvas::Stroke::default()
                .with_color(line_color)
                .with_width(1.0),
        );

        let text = canvas::Text {
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
        };
        frame.fill_text(text);
    }

    let border_color = velocity_border_color(theme);
    frame.fill_rectangle(
        Point::new(0.0, draw_top),
        Size::new(width, 1.0),
        border_color,
    );
}

/// 绘制顶部 resize 拖拽手柄
fn draw_resize_handle(frame: &mut Frame<Renderer>, theme: &Theme, size: Size, hovered: bool) {
    let handle_color = velocity_handle_bg_color(theme, hovered);
    let grab_bar_color = velocity_grab_bar_color(theme);

    // 手柄背景
    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        Size::new(size.width, RESIZE_HANDLE_HEIGHT),
        handle_color,
    );

    // 中间的小 grab 指示条
    let bar_width = 40.0;
    let bar_height = 3.0;
    let bar_x = (size.width - bar_width) / 2.0;
    let bar_y = (RESIZE_HANDLE_HEIGHT - bar_height) / 2.0;
    frame.fill_rectangle(
        Point::new(bar_x, bar_y),
        Size::new(bar_width, bar_height),
        grab_bar_color,
    );
}

/// 速度面板 BPM 标尺常量
const TEMPO_BPM_MIN: f64 = 20.0;
const TEMPO_BPM_MAX: f64 = 10000.0;

/// 将 BPM 值映射到面板 Y 坐标（绝对标尺 [20, 10000]）
fn tempo_bpm_to_y(bpm: f64, bounds_height: f32) -> f32 {
    let draw_height = bounds_height - RESIZE_HANDLE_HEIGHT;
    let max_y = draw_height - PANEL_PADDING_Y;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = ((bpm - TEMPO_BPM_MIN) / (TEMPO_BPM_MAX - TEMPO_BPM_MIN)) as f32;
    max_y - normalized * (max_y - min_y)
}

/// 生成 BPM 标尺刻度值（最大 10000，中间自适应对数分布）
fn generate_tempo_levels() -> Vec<f64> {
    vec![
        TEMPO_BPM_MIN,
        60.0,
        120.0,
        240.0,
        480.0,
        1000.0,
        2000.0,
        5000.0,
        TEMPO_BPM_MAX,
    ]
}

/// 绘制速度（Tempo）折线图
fn draw_tempo_graph(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    points: &[TempoPoint],
    size: Size,
    view: &ViewState,
) {
    if points.is_empty() {
        return;
    }

    let width = size.width;
    let height = size.height;

    let line_color = theme.extended_palette().secondary.strong.color;
    let point_color = theme.extended_palette().secondary.base.color;

    // 使用固定绝对标尺 [20, 9999]，保证单点也能正确映射位置
    let min_bpm = TEMPO_BPM_MIN;
    let bpm_range = TEMPO_BPM_MAX - TEMPO_BPM_MIN;

    // 预计算可见点屏幕坐标（保留 BPM 值用于标签）
    // Tempo 点通常很少，不需要降采样
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
            .with_width(2.0),
    );

    // 合批绘制控制点
    let mut circle_builder = path::Builder::new();
    for &(pos, _) in &screen_points {
        circle_builder.circle(pos, POINT_RADIUS);
    }
    frame.fill(&circle_builder.build(), point_color);

    // BPM 标签（Text 开销不高，不做合批）
    for &(pos, bpm) in &screen_points {
        let text = canvas::Text {
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
        };
        frame.fill_text(text);
    }
}

/// 计算 Tempo 控制点屏幕位置（绝对 BPM 标尺 [20, 10000]）
fn tempo_point_screen_pos(
    point: &TempoPoint,
    _bounds_width: f32,
    bounds_height: f32,
    view: &ViewState,
    min_bpm: f64,
    bpm_range: f64,
) -> Point {
    let x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
    let draw_height = bounds_height - RESIZE_HANDLE_HEIGHT;
    let max_y = draw_height - PANEL_PADDING_Y;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = ((point.bpm - min_bpm) / bpm_range) as f32;
    let y = max_y - normalized * (max_y - min_y);
    Point::new(x, y)
}

// draw_cc_graph / draw_bend_graph / cc_point_screen_pos / bend_point_screen_pos
// 均已迁移到 wgpu 离屏渲染（CcBarRenderer 统一矩形渲染）

/// 绘制曲线绘制模式的视觉反馈
fn draw_curve_paint_feedback(
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

    // 计算绘制区域
    let start_x = state.curve_start_x;
    let cursor_local = cursor
        .position()
        .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
    let Some(current_pos) = cursor_local else {
        return;
    };
    let current_x = current_pos.x;
    let min_x = start_x.min(current_x);
    let max_x = start_x.max(current_x);

    // 画笔轨迹区域：半透明矩形覆盖受影响范围
    let range_color = velocity_curve_range_color(theme);
    frame.fill_rectangle(
        Point::new(min_x, 0.0),
        Size::new(max_x - min_x, height),
        range_color,
    );

    // 从起始到当前绘制一条连线（画笔轨迹）
    let start_vel = state.curve_start_velocity;
    let current_vel = VelocityCanvas::y_to_velocity(current_pos.y, height);
    let start_y = VelocityCanvas::velocity_to_y(start_vel, height);
    let current_y = VelocityCanvas::velocity_to_y(current_vel, height);

    let trail_color = velocity_curve_trail_color(theme);
    let mut trail_builder = path::Builder::new();
    trail_builder.move_to(Point::new(start_x, start_y));
    trail_builder.line_to(Point::new(current_x, current_y));
    frame.stroke(
        &trail_builder.build(),
        canvas::Stroke::default()
            .with_color(trail_color)
            .with_width(2.0),
    );

    // 高亮被影响的力度点
    let affected_color = theme.extended_palette().secondary.strong.color;
    for point in points {
        if !state.curve_affected.contains_key(&point.note_index) {
            continue;
        }
        let pos = VelocityCanvas::point_screen_pos(point, 0, width, height, view);
        let glow = Color::from_rgba(affected_color.r, affected_color.g, affected_color.b, 0.4);
        frame.fill(&canvas::Path::circle(pos, POINT_RADIUS + 4.0), glow);
        frame.fill(
            &canvas::Path::circle(pos, POINT_RADIUS + 1.0),
            affected_color,
        );
    }
}

/// 绘制竖向网格线（小节线/拍线/半拍线）
///
/// 替代原来的 wgpu 离屏绘制，由 Canvas 直接渲染。
/// 逻辑与钢琴卷帘的竖向网格线一致，与 self.editor.editor_state.view 联动滚动。
fn draw_vertical_lines(frame: &mut Frame<Renderer>, theme: &Theme, size: Size, view: &ViewState) {
    let height = size.height;
    let width = size.width;

    let ppq = view.ppq as f32;
    let ticks_per_beat = ppq;
    let ticks_per_measure = ppq * 4.0;
    let visible_tick_start = view.scroll_x / view.zoom_x;
    let visible_tick_end = (view.scroll_x + width - view.keyboard_width) / view.zoom_x;

    let line_y = RESIZE_HANDLE_HEIGHT;
    let line_h = height - 2.0 * RESIZE_HANDLE_HEIGHT;

    // 小节线
    let bar_color = theme.bar_line_color();
    let measure_start = (visible_tick_start / ticks_per_measure).floor() as u32;
    let measure_end = (visible_tick_end / ticks_per_measure).ceil() as u32;
    for measure in measure_start..=measure_end {
        let tick = measure as f32 * ticks_per_measure;
        let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if x >= view.keyboard_width && x <= width {
            frame.fill_rectangle(
                Point::new(x, line_y),
                Size::new(1.0, line_h),
                Color {
                    a: 0.5,
                    ..bar_color
                },
            );
        }
    }

    // 拍线
    let beat_color = theme.beat_line_color();
    let beat_start = (visible_tick_start / ticks_per_beat).floor() as u32;
    let beat_end = (visible_tick_end / ticks_per_beat).ceil() as u32;
    for beat in beat_start..=beat_end {
        let tick = beat as f32 * ticks_per_beat;
        // 跳过小节线位置
        if (tick % ticks_per_measure as f32).abs() < f32::EPSILON {
            continue;
        }
        let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
        if x >= view.keyboard_width && x <= width {
            frame.fill_rectangle(
                Point::new(x, line_y),
                Size::new(1.0, line_h),
                Color {
                    a: 0.3,
                    ..beat_color
                },
            );
        }
    }

    // 半拍线（zoom 足够大时才显示，避免过密）
    if view.zoom_x > 0.05 {
        let half_beat_color = theme.half_beat_line_color();
        let ticks_per_half_beat = ppq / 2.0;
        let half_beat_start = (visible_tick_start / ticks_per_half_beat).floor() as u32;
        let half_beat_end = (visible_tick_end / ticks_per_half_beat).ceil() as u32;
        for hb in half_beat_start..=half_beat_end {
            let tick = hb as f32 * ticks_per_half_beat;
            // 跳过小节线和拍线位置
            if (tick % ticks_per_measure as f32).abs() < f32::EPSILON
                || (tick % ticks_per_beat).abs() < f32::EPSILON
            {
                continue;
            }
            let x = tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if x >= view.keyboard_width && x <= width {
                frame.fill_rectangle(
                    Point::new(x, line_y),
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

/// 绘制横向参考线（Velocity/CC/Bend 模式）
///
/// 替代原来的 wgpu 离屏绘制，由 Canvas 直接渲染。
/// 刻度值位置与 draw_scale_labels 保持一致。
fn draw_horizontal_lines(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    size: Size,
    edit_mode: super::EditMode,
) {
    let width = size.width;
    let line_color = velocity_grid_line_color(theme);

    match edit_mode {
        super::EditMode::Velocity | super::EditMode::Cc(_) => {
            let scale_values = [0u8, 32, 64, 96, 127];
            for &v in &scale_values {
                let y = VelocityCanvas::velocity_to_y(v, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                let line_path = line_builder.build();
                frame.stroke(
                    &line_path,
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
        super::EditMode::Bend => {
            let bend_values: [i16; 5] = [-8192, -4096, 0, 4096, 8191];
            for &v in &bend_values {
                let y = bend_value_to_y(v, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                let line_path = line_builder.build();
                frame.stroke(
                    &line_path,
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
        super::EditMode::Tempo => {
            let bpm_levels = generate_tempo_levels();
            for &bpm in &bpm_levels {
                let y = tempo_bpm_to_y(bpm, size.height);
                let mut line_builder = path::Builder::new();
                line_builder.move_to(Point::new(PANEL_PADDING_X, y));
                line_builder.line_to(Point::new(width - PANEL_PADDING_X, y));
                let line_path = line_builder.build();
                frame.stroke(
                    &line_path,
                    canvas::Stroke::default()
                        .with_color(line_color)
                        .with_width(1.0),
                );
            }
        }
    }
}

/// 绘制刻度标签文字（Velocity/CC/Bend/Tempo 模式）
///
/// 在图形区域左侧绘制数值/刻度标签，与 wgpu 的水平刻度线配合使用。
/// 文字颜色与现有 Canvas 网格标签一致。
fn draw_scale_labels(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    size: Size,
    edit_mode: super::EditMode,
) {
    let text_color = velocity_text_color(theme);
    let width = size.width;

    match edit_mode {
        super::EditMode::Velocity | super::EditMode::Cc(_) => {
            let scale_values = [0u8, 32, 64, 96, 127];
            for &v in &scale_values {
                let y = VelocityCanvas::velocity_to_y(v, size.height);
                let text = canvas::Text {
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
                };
                frame.fill_text(text);
            }
        }
        super::EditMode::Bend => {
            let bend_labels: [(i16, &str); 5] = [
                (-8192, "-8k"),
                (-4096, "-4k"),
                (0, "0"),
                (4096, "+4k"),
                (8191, "+8k"),
            ];
            for &(v, label) in &bend_labels {
                let y = bend_value_to_y(v, size.height);
                let text = canvas::Text {
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
                };
                frame.fill_text(text);
            }
        }
        super::EditMode::Tempo => {
            let bpm_levels = generate_tempo_levels();
            let text_color = velocity_text_color(theme);
            let width = size.width;
            for &bpm in &bpm_levels {
                let y = tempo_bpm_to_y(bpm, size.height);
                let label = format!("{:.0}", bpm);
                let text = canvas::Text {
                    content: label,
                    position: Point::new(4.0, y - 6.0),
                    max_width: width,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(9.0),
                    color: text_color,
                    font: iced_core::Font::DEFAULT,
                    align_x: alignment::Horizontal::Left.into(),
                    align_y: alignment::Vertical::Top,
                    shaping: iced_core::text::Shaping::Basic,
                };
                frame.fill_text(text);
            }
        }
    }
}

/// 将弯音值 (-8192 ~ +8191) 映射到面板 Y 坐标
fn bend_value_to_y(value: i16, bounds_height: f32) -> f32 {
    let draw_height = bounds_height - RESIZE_HANDLE_HEIGHT;
    let max_y = draw_height - PANEL_PADDING_Y;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let normalized = (value as f32 + 8192.0) / 16383.0;
    max_y - normalized * (max_y - min_y)
}
