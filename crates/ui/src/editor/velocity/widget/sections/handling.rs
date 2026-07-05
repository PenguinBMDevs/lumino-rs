//! 事件处理方法：鼠标交互处理、曲线绘制更新

use iced_core::{Point, Size, mouse};
use iced_widget::canvas;

use crate::Message;
use crate::editor::editor_state::ViewState;
use crate::message::VelocityAction;

use super::super::super::{
    RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT, VELOCITY_PANEL_MAX_HEIGHT, VELOCITY_PANEL_MIN_HEIGHT,
    VelocityPanel, VelocityPoint,
};
use super::super::state::VelocityCanvasState;

impl<'a> super::super::VelocityCanvas<'a> {
    pub(super) fn handle_button_pressed(
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

    pub(super) fn handle_cursor_moved(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        cursor: &mouse::Cursor,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if state.resize_dragging {
            let abs_cursor_y = cursor.position().unwrap_or_default().y;
            let delta_y = state.resize_drag_start_y - abs_cursor_y;
            let new_height = (state.resize_start_height + delta_y)
                .clamp(VELOCITY_PANEL_MIN_HEIGHT, VELOCITY_PANEL_MAX_HEIGHT);
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

    pub(super) fn handle_button_released(
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
