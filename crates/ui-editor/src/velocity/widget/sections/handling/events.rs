//! 鼠标/键盘事件主入口
//!
//! 根据编辑模式分发到对应子模块的处理方法。

use iced_core::{Point, Size, keyboard, mouse};
use iced_widget::canvas;

use crate::velocity::EditMode;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::super::{
    TOOLBAR_HEIGHT, VELOCITY_PANEL_MAX_HEIGHT, VELOCITY_PANEL_MIN_HEIGHT, VelocityPanel,
};
use super::super::super::state::{AutomationDrag, VelocityCanvasState};
use super::publish_velocity;

impl<'a> super::super::super::VelocityCanvas<'a> {
    pub(crate) fn handle_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        cursor: &mouse::Cursor,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        // 检查 resize 手柄区域
        if Self::is_in_resize_zone(cursor_pos) {
            state.resize_dragging = true;
            state.resize_drag_start_y = cursor.position().unwrap_or_default().y;
            state.resize_start_height = bounds_size.height + TOOLBAR_HEIGHT;
            return None;
        }

        // 按编辑模式分发
        match self.edit_mode {
            EditMode::Velocity => {
                return self.handle_velocity_button_pressed(state, cursor_pos, bounds_size);
            }
            EditMode::Tempo => {
                return self.handle_tempo_button_pressed(state, cursor_pos, bounds_size);
            }
            _ => {}
        }

        self.handle_automation_button_pressed(state, cursor_pos, bounds_size)
    }

    pub(crate) fn handle_right_button_pressed(
        &self,
        _state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        // Velocity 模式：右键无操作
        if self.edit_mode == EditMode::Velocity {
            return None;
        }

        // Tempo 模式：右键点击删除速度点
        if self.edit_mode == EditMode::Tempo {
            return self.handle_tempo_right_click_delete(cursor_pos, bounds_size);
        }

        // CC / Bend 自动化模式：右键删除锚点
        self.handle_automation_right_click_delete(cursor_pos, bounds_size)
    }

    /// 处理 Tempo 模式右键删除
    fn handle_tempo_right_click_delete(
        &self,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        let view = &self.editor.editor_state.view;
        if let Some(idx) = Self::hit_test_tempo_point(
            &tempo_points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
        ) {
            return Some(publish_velocity(VelocityAction::TempoDelete(idx)));
        }
        None
    }

    /// 处理 自动化模式右键删除锚点
    fn handle_automation_right_click_delete(
        &self,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let (view, target, max_val) = self.automation_view_params(bounds_size)?;
        let track_idx = self.editor.editor_state.data.current_track as u16;
        let lane_idx = self
            .editor
            .editor_state
            .data
            .find_automation_lane(track_idx, &target);
        let lane_idx = lane_idx?;
        let lane = self
            .editor
            .editor_state
            .data
            .automation_lanes
            .get(lane_idx)?;
        if let Some(tick) = Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val) {
            return Some(publish_velocity(VelocityAction::AutomationEdit(
                lumino_core::AutomationEdit::Delete {
                    track_idx,
                    lane_idx,
                    tick,
                },
            )));
        }
        None
    }

    pub(crate) fn handle_cursor_moved(
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

        // 自动化拖拽优先处理
        if let Some(drag) = state.automation_drag {
            return self.handle_automation_cursor_moved(state, drag, cursor_pos, bounds_size);
        }

        // 力度曲线绘制模式
        if state.curve_active {
            return self.handle_velocity_curve_moved(state, cursor_pos, bounds_size);
        }

        // 力度点拖拽
        if let Some(drag_idx) = state.drag_point_idx {
            return self.handle_velocity_drag_move(state, drag_idx, cursor_pos, bounds_size);
        }

        // Tempo 点拖拽
        if let Some(drag_idx) = state.tempo_drag_idx {
            return self.handle_tempo_drag_move(state, drag_idx, cursor_pos, bounds_size);
        }

        // 更新悬停状态
        self.update_hover_state(state, cursor_pos, bounds_size);
        None
    }

    pub(crate) fn handle_button_released(
        &self,
        state: &mut VelocityCanvasState,
    ) -> Option<canvas::Action<Message>> {
        if state.resize_dragging {
            state.resize_dragging = false;
            return None;
        }

        if state.automation_drag.is_some() {
            state.reset_automation_drag();
            return None;
        }

        if state.curve_active {
            state.curve_active = false;
            state.curve_affected.clear();
            return Some(publish_velocity(VelocityAction::CurveEnd));
        }

        let was_dragging = state.drag_point_idx.is_some();
        state.drag_point_idx = None;
        state._drag_start_velocity = 0;
        if was_dragging {
            return Some(publish_velocity(VelocityAction::DragEnd));
        }

        let was_tempo_dragging = state.tempo_drag_idx.is_some();
        state.tempo_drag_idx = None;
        if was_tempo_dragging {
            return Some(publish_velocity(VelocityAction::TempoDragEnd));
        }
        None
    }

    pub(crate) fn handle_wheel_scrolled(
        &self,
        state: &VelocityCanvasState,
        delta: mouse::ScrollDelta,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        // Velocity/Tempo 模式：滚轮无操作
        if self.edit_mode == EditMode::Velocity || self.edit_mode == EditMode::Tempo {
            return None;
        }

        let (_view, _target, max_val) = self.automation_view_params(bounds_size)?;
        let delta_y = match delta {
            mouse::ScrollDelta::Lines { y, .. } => y,
            mouse::ScrollDelta::Pixels { y, .. } => y / 50.0,
        };
        if delta_y == 0.0 {
            return None;
        }

        if state.modifiers.control() {
            let zoom_delta = 1.0 + delta_y * 0.1;
            return Some(publish_velocity(VelocityAction::AutomationZoom(zoom_delta)));
        }

        let scroll_amount = -delta_y * max_val * 0.05;
        Some(publish_velocity(VelocityAction::AutomationScroll(
            scroll_amount,
        )))
    }

    pub(crate) fn handle_modifiers_changed(
        state: &mut VelocityCanvasState,
        modifiers: keyboard::Modifiers,
    ) {
        state.modifiers = modifiers;
    }
}
