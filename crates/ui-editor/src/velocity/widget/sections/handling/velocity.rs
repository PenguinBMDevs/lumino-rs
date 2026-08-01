//! Velocity 模式事件处理
//!
//! 包含力度点点击、拖拽、曲线绘制等逻辑。

use iced_core::{Point, Size};
use iced_widget::canvas;

use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::super::RESIZE_HANDLE_HEIGHT;
use super::super::super::state::VelocityCanvasState;
use super::publish_velocity;

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// 处理 Velocity 模式下的按钮点击
    pub(super) fn handle_velocity_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let points = self.points();
        let view = &self.editor.editor_state.view;
        if points.is_empty() {
            return None;
        }

        // 点击已有锚点：开始拖拽
        if let Some(point_idx) = Self::hit_test(
            &points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
        ) {
            state.drag_point_idx = Some(point_idx);
            state._drag_start_velocity = points[point_idx].velocity;
            return Some(publish_velocity(VelocityAction::DragStart(
                points[point_idx].note_index,
                points[point_idx].velocity,
            )));
        }

        // 检查是否在绘制区域内
        let in_draw_area = cursor_pos.x >= 0.0
            && cursor_pos.x <= bounds_size.width
            && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
            && cursor_pos.y <= bounds_size.height;
        if !in_draw_area {
            return None;
        }

        // 空白处点击：进入曲线绘制模式
        state.curve_active = true;
        state.curve_start_x = cursor_pos.x;
        state.curve_start_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
        state.curve_affected.clear();
        state.drag_point_idx = None;
        state.hover_point_idx = None;
        Some(publish_velocity(VelocityAction::CurveStart))
    }

    /// 处理 Velocity 曲线绘制拖拽移动
    pub(super) fn handle_velocity_curve_moved(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let out_of_bounds = cursor_pos.x < 0.0
            || cursor_pos.x > bounds_size.width
            || cursor_pos.y < RESIZE_HANDLE_HEIGHT
            || cursor_pos.y > bounds_size.height;
        if out_of_bounds {
            state.curve_active = false;
            state.curve_affected.clear();
            return Some(publish_velocity(VelocityAction::CurveEnd));
        }

        let points = self.points();
        if points.is_empty() {
            return None;
        }

        let view = &self.editor.editor_state.view;
        let has_selection = self.editor.has_selection();
        let is_selected = |idx: usize| self.editor.is_note_selected(idx);
        Self::update_curve_paint(
            state,
            &points,
            cursor_pos,
            bounds_size,
            view,
            has_selection,
            &is_selected,
        )
    }

    /// 处理 Velocity 力度点拖拽移动
    pub(super) fn handle_velocity_drag_move(
        &self,
        _state: &mut VelocityCanvasState,
        drag_idx: usize,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let points = self.points();
        if drag_idx < points.len() {
            let new_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
            let old_velocity = points[drag_idx].velocity;
            if new_velocity != old_velocity {
                return Some(publish_velocity(VelocityAction::DragMove(
                    points[drag_idx].note_index,
                    new_velocity,
                )));
            }
        }
        None
    }
}
