//! 力度/CC/Tempo Canvas 程序
//!
//! 包含 Canvas Program trait 实现和事件处理逻辑。

mod drawing;
mod state;

pub use drawing::{
    bend_value_to_y, draw_background, draw_curve_paint_feedback, draw_horizontal_lines,
    draw_resize_handle, draw_scale_labels, draw_tempo_graph, draw_vertical_lines,
    generate_tempo_levels, tempo_bpm_to_y, tempo_point_screen_pos, velocity_bg_color,
    velocity_border_color, velocity_grab_bar_color, velocity_grid_line_color,
    velocity_handle_bg_color, velocity_text_color,
};
pub use state::VelocityCanvasState;

/// 速度控制点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoPoint {
    /// tick 位置
    pub tick: f32,
    /// BPM 值 (20-10000)
    pub bpm: f64,
}

use iced_core::{Point, Rectangle, Size, mouse};
use iced_widget::canvas::{self, Frame, Program};

use crate::editor::editor_state::ViewState;
use crate::message::VelocityAction;
use crate::{Message, Renderer, Theme};

use super::{
    EditMode, HIT_RADIUS, PANEL_PADDING_X, PANEL_PADDING_Y, RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT,
    VelocityPanel, VelocityPoint,
};

// iced_wgpu::Geometry is the concrete canvas geometry type for the wgpu backend
use iced_wgpu::Geometry as Geom;

/// 力度/CC Canvas 程序
pub struct VelocityCanvas<'a> {
    pub editor: &'a crate::editor::Editor,
    /// 当前编辑模式
    pub edit_mode: EditMode,
    /// CC 模式下选择的控制器编号
    pub selected_cc: u8,
}

impl<'a> VelocityCanvas<'a> {
    /// 获取所有力度点
    fn points(&self) -> Vec<VelocityPoint> {
        let notes = &self.editor.editor_state.data.notes;
        VelocityPanel::build_velocity_points(notes)
    }

    /// 将力度值映射到 Y 坐标
    pub fn velocity_to_y(velocity: u8, bounds_height: f32) -> f32 {
        let max_y = bounds_height;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let normalized = velocity as f32 / 127.0;
        max_y - normalized * (max_y - min_y)
    }

    /// 将 Y 坐标映射回力度值 (0-127)
    pub fn y_to_velocity(y: f32, bounds_height: f32) -> u8 {
        let max_y = bounds_height;
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
        let clamped_y = y.clamp(min_y, max_y);
        let normalized = (max_y - clamped_y) / (max_y - min_y);
        (normalized * 127.0).round().clamp(0.0, 127.0) as u8
    }

    /// 获取点的屏幕位置
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

    fn handle_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        cursor: &mouse::Cursor,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if Self::is_in_resize_zone(cursor_pos) {
            state.resize_dragging = true;
            state.resize_drag_start_y = cursor.position().unwrap_or_default().y;
            state.resize_start_height = bounds_size.height + TOOLBAR_HEIGHT;
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
                VelocityAction::DragStart(points[point_idx].note_index, points[point_idx].velocity),
            )));
        }

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

    fn handle_cursor_moved(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        cursor: &mouse::Cursor,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if state.resize_dragging {
            let abs_cursor_y = cursor.position().unwrap_or_default().y;
            let delta_y = state.resize_drag_start_y - abs_cursor_y;
            let new_height = (state.resize_start_height + delta_y).clamp(
                super::VELOCITY_PANEL_MIN_HEIGHT,
                super::VELOCITY_PANEL_MAX_HEIGHT,
            );
            let current_panel_height = bounds_size.height + TOOLBAR_HEIGHT;
            if (new_height - current_panel_height).abs() > 1.0 {
                return Some(canvas::Action::publish(Message::VelocityPanelResize(
                    new_height,
                )));
            }
            return None;
        }

        state.hover_resize_handle = Self::is_in_resize_zone(cursor_pos);

        if state.curve_active {
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
                    return Some(canvas::Action::publish(Message::Velocity(
                        VelocityAction::DragMove(points[drag_idx].note_index, new_velocity),
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

    fn handle_button_released(
        &self,
        state: &mut VelocityCanvasState,
    ) -> Option<canvas::Action<Message>> {
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

    /// 更新曲线绘制
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
        let has_selection = !selected_notes.is_empty();

        let mut updates: Vec<(usize, u8)> = Vec::new();

        for point in points {
            let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if point_x < min_x || point_x > max_x {
                continue;
            }
            if has_selection && !selected_notes.contains(&point.note_index) {
                continue;
            }

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
                self.handle_button_pressed(state, cursor_pos, &cursor, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                self.handle_cursor_moved(state, cursor_pos, &cursor, bounds_size)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                self.handle_button_released(state)
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
            EditMode::Tempo => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
                let tempo_points = VelocityPanel::build_tempo_points(self.editor);
                if !tempo_points.is_empty() {
                    draw_tempo_graph(&mut frame, theme, &tempo_points, bounds.size(), view);
                }
                vec![frame.into_geometry()]
            }
            _ => {
                let mut frame = Frame::new(renderer, bounds.size());
                let view = &self.editor.editor_state.view;
                draw_vertical_lines(&mut frame, theme, bounds.size(), view);
                draw_horizontal_lines(&mut frame, theme, bounds.size(), self.edit_mode);
                draw_scale_labels(&mut frame, theme, bounds.size(), self.edit_mode);
                vec![frame.into_geometry()]
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
