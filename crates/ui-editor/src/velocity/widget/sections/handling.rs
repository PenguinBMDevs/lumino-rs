//! 事件处理方法：鼠标交互处理、曲线绘制更新

use iced_core::{Point, Size, keyboard, mouse};
use iced_widget::canvas;
use lumino_core::{AutomationEdit, SegmentShape, Tool};

use crate::editor_state::ViewState;
use crate::velocity::EditMode;
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::{
    RESIZE_HANDLE_HEIGHT, TOOLBAR_HEIGHT, VELOCITY_PANEL_MAX_HEIGHT, VELOCITY_PANEL_MIN_HEIGHT,
    VelocityPanel, VelocityPoint,
};
use super::super::state::{AutomationDrag, CtrlEnd, VelocityCanvasState};

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

        match self.edit_mode {
            EditMode::Velocity => {
                return self.handle_velocity_button_pressed(state, cursor_pos, bounds_size);
            }
            EditMode::Tempo => {
                return self.handle_tempo_button_pressed(state, cursor_pos, bounds_size);
            }
            _ => {}
        }

        // CC / Bend 自动化模式
        let (view, target, max_val) = self.automation_view_params(bounds_size)?;
        let in_draw_area = cursor_pos.x >= 0.0
            && cursor_pos.x <= bounds_size.width
            && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
            && cursor_pos.y <= bounds_size.height;
        if !in_draw_area {
            return None;
        }

        let track_idx = self.editor.editor_state.data.current_track as u16;
        let lane_idx = self
            .editor
            .editor_state
            .data
            .find_automation_lane(track_idx, &target);
        let lane_ref =
            lane_idx.and_then(|idx| self.editor.editor_state.data.automation_lanes.get(idx));

        // 双击切换 shape
        if state.detect_double_click(cursor_pos) {
            if let Some(lane) = lane_ref
                && let Some(tick) =
                    Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val)
                && let Some(lane_idx) = lane_idx
            {
                return Some(publish_velocity(VelocityAction::AutomationEdit(
                    AutomationEdit::CycleShape {
                        track_idx,
                        lane_idx,
                        tick,
                    },
                )));
            }
            return None;
        }

        match self.editor.current_tool() {
            Tool::Eraser => {
                if let Some(lane) = lane_ref
                    && let Some(tick) =
                        Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val)
                    && let Some(lane_idx) = lane_idx
                {
                    return Some(publish_velocity(VelocityAction::AutomationEdit(
                        AutomationEdit::Delete {
                            track_idx,
                            lane_idx,
                            tick,
                        },
                    )));
                }
            }
            Tool::Pencil | Tool::Pointer => {
                // 先检测贝塞尔控制点命中
                if let Some(lane) = lane_ref
                    && let Some((prev_tick, which, x1, y1, x2, y2)) =
                        Self::hit_test_control_point(lane, &view, cursor_pos, max_val)
                {
                    let (start_x, start_y) = match which {
                        CtrlEnd::Out => (x1, y1),
                        CtrlEnd::In => (x2, y2),
                    };
                    state.start_drag_control_point(prev_tick, which, start_x, start_y);
                    return Some(publish_velocity(VelocityAction::AutomationDragStart));
                }
                // Pencil/Pointer 只允许拖拽已有锚点，禁止在空白处创建新锚点
                // 创建 CC 锚点请使用 Curve 曲线编辑工具
                if let Some(lane) = lane_ref
                    && let Some(tick) =
                        Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val)
                {
                    state.start_move_anchor(tick);
                    state.automation_curve_current = None;
                    return Some(publish_velocity(VelocityAction::AutomationDragStart));
                }
            }
            Tool::Curve => {
                let tick = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0) as u32;
                let value = view
                    .y_to_value(cursor_pos.y, max_val)
                    .round()
                    .clamp(0.0, max_val) as u16;
                state.start_curve_draw(tick, value);
                return Some(publish_velocity(VelocityAction::AutomationDragStart));
            }
            _ => {}
        }

        None
    }

    pub(super) fn handle_right_button_pressed(
        &self,
        _state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if self.edit_mode == EditMode::Velocity {
            return None;
        }
        // Tempo 模式：右键点击删除速度点
        if self.edit_mode == EditMode::Tempo {
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
            return None;
        }
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
                AutomationEdit::Delete {
                    track_idx,
                    lane_idx,
                    tick,
                },
            )));
        }
        None
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

    pub(super) fn handle_button_released(
        &self,
        state: &mut VelocityCanvasState,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if state.resize_dragging {
            state.resize_dragging = false;
            return None;
        }

        // CurveDraw 在释放时提交编辑（参考 yinhe 模式）
        if let Some(AutomationDrag::CurveDraw {
            start_tick,
            start_value,
        }) = state.automation_drag
        {
            return self.commit_curve_draw(state, start_tick, start_value, bounds_size);
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

    /// CurveDraw 释放时提交编辑（参考 yinhe 模式）
    fn commit_curve_draw(
        &self,
        state: &mut VelocityCanvasState,
        start_tick: u32,
        start_value: u16,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        // 先捕获 ghost 位置，再 reset（reset 会清空 automation_curve_current）
        let ghost = state.automation_curve_current;
        state.reset_automation_drag();
        let (current_tick, current_value) = ghost?;
        let (_, target, _) = self.automation_view_params(bounds_size)?;
        let track_idx = self.editor.editor_state.data.current_track as u16;
        if current_tick == start_tick {
            // 单击：只创建一个 linear_curve() 锚点
            return Some(publish_velocity(VelocityAction::AutomationEdit(
                AutomationEdit::Add {
                    track_idx,
                    target: target.clone(),
                    channel: 0,
                    tick: current_tick,
                    value: current_value,
                    shape: SegmentShape::linear_curve(),
                },
            )));
        }
        // 拖拽：创建 2 个锚点（起点 linear_curve() + 终点 Step）
        let (t1, v1, t2, v2) = if start_tick < current_tick {
            (start_tick, start_value, current_tick, current_value)
        } else {
            (current_tick, current_value, start_tick, start_value)
        };
        Some(publish_velocity(VelocityAction::AutomationBatch(vec![
            AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick: t1,
                value: v1,
                shape: SegmentShape::linear_curve(),
            },
            AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick: t2,
                value: v2,
                shape: SegmentShape::Step,
            },
        ])))
    }

    pub(super) fn handle_wheel_scrolled(
        &self,
        state: &VelocityCanvasState,
        delta: mouse::ScrollDelta,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
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

    pub(super) fn handle_modifiers_changed(
        state: &mut VelocityCanvasState,
        modifiers: keyboard::Modifiers,
    ) {
        state.modifiers = modifiers;
    }

    // ── Velocity 模式 ──

    fn handle_velocity_button_pressed(
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
        Some(publish_velocity(VelocityAction::CurveStart))
    }

    fn handle_velocity_curve_moved(
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

    fn handle_velocity_drag_move(
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

    // ── Tempo 模式 ──

    fn handle_tempo_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        let view = &self.editor.editor_state.view;
        let hit_idx = Self::hit_test_tempo_point(
            &tempo_points,
            cursor_pos,
            bounds_size.width,
            bounds_size.height,
            view,
        );

        let in_draw_area = cursor_pos.x >= 0.0
            && cursor_pos.x <= bounds_size.width
            && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
            && cursor_pos.y <= bounds_size.height;
        if !in_draw_area {
            return None;
        }

        match self.editor.current_tool() {
            Tool::Eraser => {
                if let Some(idx) = hit_idx {
                    return Some(publish_velocity(VelocityAction::TempoDelete(idx)));
                }
                None
            }
            Tool::Pencil => {
                if let Some(idx) = hit_idx {
                    // 点击已有锚点：开始拖拽
                    state.tempo_drag_idx = Some(idx);
                    Some(publish_velocity(VelocityAction::TempoDragStart(idx)))
                } else {
                    // 在空白处创建新点（吸附到网格）
                    let tick = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
                    let bpm = Self::y_to_bpm(cursor_pos.y, bounds_size.height);
                    Some(publish_velocity(VelocityAction::TempoAdd(tick, bpm)))
                }
            }
            Tool::Pointer => {
                if let Some(idx) = hit_idx {
                    // 点击已有锚点：开始拖拽
                    state.tempo_drag_idx = Some(idx);
                    Some(publish_velocity(VelocityAction::TempoDragStart(idx)))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn handle_tempo_drag_move(
        &self,
        _state: &mut VelocityCanvasState,
        drag_idx: usize,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let tempo_points = VelocityPanel::build_tempo_points(self.editor);
        if drag_idx < tempo_points.len() {
            let _tick = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
            let bpm = Self::y_to_bpm(cursor_pos.y, bounds_size.height).clamp(20.0, 10000.0);
            // 更新 BPM（tick 拖拽由 TempoAdd+TempoDelete 模式实现）
            return Some(publish_velocity(VelocityAction::TempoDragMove(
                drag_idx, bpm,
            )));
        }
        None
    }

    // ── Automation 模式 ──

    fn handle_automation_cursor_moved(
        &self,
        state: &mut VelocityCanvasState,
        drag: AutomationDrag,
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

        match drag {
            AutomationDrag::MoveAnchor { old_tick } => {
                let lane_idx = lane_idx?;
                let new_tick_f = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
                let new_tick = new_tick_f as u32;
                let new_value = view
                    .y_to_value(cursor_pos.y, max_val)
                    .round()
                    .clamp(0.0, max_val) as u16;
                state.automation_curve_current = Some((new_tick, new_value));
                if new_tick == old_tick {
                    // 仅更新 value
                    let edit = AutomationEdit::Move {
                        track_idx,
                        lane_idx,
                        old_tick,
                        new_tick,
                        new_value,
                    };
                    return Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                        edit,
                    ])));
                }
                // 移动到新 tick：先删除旧事件再添加新事件，避免同一 tick 冲突
                let edits = vec![
                    AutomationEdit::Delete {
                        track_idx,
                        lane_idx,
                        tick: old_tick,
                    },
                    AutomationEdit::Add {
                        track_idx,
                        target: target.clone(),
                        channel: 0,
                        tick: new_tick,
                        value: new_value,
                        shape: target.default_shape(),
                    },
                ];
                // 拖拽过程中把锚点视为已移动到新位置，便于连续拖拽
                state.automation_drag = Some(AutomationDrag::MoveAnchor { old_tick: new_tick });
                Some(publish_velocity(VelocityAction::AutomationBatch(edits)))
            }
            AutomationDrag::CurveDraw { .. } => {
                // CurveDraw 移动阶段只更新 ghost 预览，不提交编辑
                // 编辑在 button_released 时提交（参考 yinhe 模式）
                let current_tick_f = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
                let current_tick = current_tick_f as u32;
                let current_value = view
                    .y_to_value(cursor_pos.y, max_val)
                    .round()
                    .clamp(0.0, max_val) as u16;
                state.automation_curve_current = Some((current_tick, current_value));
                None
            }
            AutomationDrag::DragControlPoint {
                prev_tick,
                which,
                start_x,
                start_y,
            } => {
                let lane_idx = lane_idx?;
                let lane = self
                    .editor
                    .editor_state
                    .data
                    .automation_lanes
                    .get(lane_idx)?;
                // 从鼠标反推控制点偏移量
                let (new_x, new_y) = Self::compute_ctrl_from_mouse(
                    lane, prev_tick, which, cursor_pos, &view, max_val,
                )?;
                // 如果没有实际移动，不提交
                if (new_x - start_x).abs() < 1e-4 && (new_y - start_y).abs() < 1e-4 {
                    return None;
                }
                // 合并到 shape
                let new_shape = Self::merge_ctrl_shape(lane, prev_tick, which, (new_x, new_y));
                // 更新 ghost
                state.drag_ctrl_ghost = Some((prev_tick, which, new_shape));
                // 提交 SetShape 编辑（走 Batch，不重复 push history）
                let edit = AutomationEdit::SetShape {
                    track_idx,
                    lane_idx,
                    tick: prev_tick,
                    shape: new_shape,
                };
                Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                    edit,
                ])))
            }
        }
    }

    fn update_hover_state(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) {
        match self.edit_mode {
            EditMode::Velocity => {
                let points = self.points();
                let view = &self.editor.editor_state.view;
                if points.is_empty() {
                    state.hover_point_idx = None;
                    return;
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
                state.hover_anchor_tick = None;
            }
            EditMode::Tempo => {
                let tempo_points = VelocityPanel::build_tempo_points(self.editor);
                let view = &self.editor.editor_state.view;
                if let Some(idx) = Self::hit_test_tempo_point(
                    &tempo_points,
                    cursor_pos,
                    bounds_size.width,
                    bounds_size.height,
                    view,
                ) {
                    state.tempo_hover_idx = Some(idx);
                } else {
                    state.tempo_hover_idx = None;
                }
                state.hover_point_idx = None;
                state.hover_anchor_tick = None;
            }
            _ => {
                if let Some((view, _target, max_val)) = self.automation_view_params(bounds_size)
                    && let Some(lane) = self.current_automation_lane()
                {
                    state.hover_anchor_tick =
                        Self::hit_test_automation_anchor(lane, &view, cursor_pos, max_val);
                }
                state.hover_point_idx = None;
            }
        }
    }

    /// 更新曲线绘制
    fn update_curve_paint(
        state: &mut VelocityCanvasState,
        points: &[VelocityPoint],
        cursor_pos: Point,
        bounds_size: Size,
        view: &ViewState,
        has_selection: bool,
        is_selected: &dyn Fn(usize) -> bool,
    ) -> Option<canvas::Action<Message>> {
        let start_x = state.curve_start_x;
        let current_x = cursor_pos.x;
        let min_x = start_x.min(current_x);
        let max_x = start_x.max(current_x);
        let current_velocity = Self::y_to_velocity(cursor_pos.y, bounds_size.height);
        let start_velocity = state.curve_start_velocity;

        let mut updates: Vec<(usize, u8)> = Vec::new();

        for point in points {
            let point_x = point.tick * view.zoom_x - view.scroll_x + view.keyboard_width;
            if point_x < min_x || point_x > max_x {
                continue;
            }
            if has_selection && !is_selected(point.note_index) {
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
        Some(publish_velocity(VelocityAction::CurvePaint(updates)))
    }
}

fn publish_velocity(action: VelocityAction) -> canvas::Action<Message> {
    canvas::Action::publish(Message::Velocity(action))
}
