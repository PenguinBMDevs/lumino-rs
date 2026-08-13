//! CC/Bend 自动化模式事件处理
//!
//! 包含自动化锚点的点击、拖拽、曲线绘制等逻辑。

use iced_core::{Point, Size};
use iced_widget::canvas;
use lumino_core::Tool;
use lumino_note_core::AutomationEdit;

use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::super::RESIZE_HANDLE_HEIGHT;
use super::super::super::state::{AutomationDrag, VelocityCanvasState};
use super::publish_velocity;

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// 处理 CC/Bend 自动化模式下的按钮点击
    pub(super) fn handle_automation_button_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let (view, target, max_val) = self.automation_view_params(bounds_size)?;
        let in_draw_area = cursor_pos.x >= 0.0
            && cursor_pos.x <= bounds_size.width
            && cursor_pos.y >= RESIZE_HANDLE_HEIGHT
            && cursor_pos.y <= bounds_size.height;
        if !in_draw_area {
            return None;
        }

        // Bend 模式 Curve 工具：贝塞尔路径交互（√× 按钮/双击删除/绘制编辑）
        if self.edit_mode == crate::velocity::EditMode::Bend
            && self.editor.current_tool() == Tool::Curve
        {
            return self.handle_bend_curve_pressed(state, cursor_pos, bounds_size);
        }

        let track_idx = self.editor.editor_state.data.current_track as u16;
        let lane_idx = self
            .editor
            .editor_state
            .data
            .find_automation_lane(track_idx, &target);
        let lane_ref =
            lane_idx.and_then(|idx| self.editor.editor_state.data.automation_lanes.get(idx));

        // 双击切换 shape（仅 Curve 工具可编辑自动化面板）
        if self.editor.current_tool() == Tool::Curve && state.detect_double_click(cursor_pos) {
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

        self.handle_automation_tool_action(
            state,
            cursor_pos,
            &view,
            max_val,
            lane_ref.map(|v| &**v),
            lane_idx.map(|v| v as u16),
            track_idx,
        )
    }

    /// 根据当前工具执行自动化操作
    fn handle_automation_tool_action(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        view: &lumino_gfx::automation::AutomationViewParams,
        max_val: f32,
        lane_ref: Option<&lumino_note_core::AutomationLane>,
        lane_idx: Option<u16>,
        track_idx: u16,
    ) -> Option<canvas::Action<Message>> {
        match self.editor.current_tool() {
            Tool::Eraser => self.handle_automation_eraser_delete(
                lane_ref, lane_idx, track_idx, view, cursor_pos, max_val,
            ),
            // 自动化面板的编辑交互统一由 Curve 工具负责：
            // 命中锚点 → 拖拽移动；未命中 → 曲线绘制。
            // Pencil/Pointer 等其他工具不操作自动化面板（仅在钢琴卷帘使用）。
            Tool::Curve => self
                .handle_automation_anchor_drag_start(state, lane_ref, view, cursor_pos, max_val)
                .or_else(|| self.handle_automation_curve_start(state, view, cursor_pos, max_val)),
            _ => None,
        }
    }

    /// Eraser 工具：删除命中的自动化锚点
    fn handle_automation_eraser_delete(
        &self,
        lane_ref: Option<&lumino_note_core::AutomationLane>,
        lane_idx: Option<u16>,
        track_idx: u16,
        view: &lumino_gfx::automation::AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> Option<canvas::Action<Message>> {
        if let Some(lane) = lane_ref
            && let Some(tick) = Self::hit_test_automation_anchor(lane, view, cursor_pos, max_val)
            && let Some(lane_idx) = lane_idx
        {
            return Some(publish_velocity(VelocityAction::AutomationEdit(
                AutomationEdit::Delete {
                    track_idx,
                    lane_idx: lane_idx as usize,
                    tick,
                },
            )));
        }
        None
    }

    /// Curve 工具：开始拖拽命中的自动化锚点
    fn handle_automation_anchor_drag_start(
        &self,
        state: &mut VelocityCanvasState,
        lane_ref: Option<&lumino_note_core::AutomationLane>,
        view: &lumino_gfx::automation::AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> Option<canvas::Action<Message>> {
        if let Some(lane) = lane_ref
            && let Some(tick) = Self::hit_test_automation_anchor(lane, view, cursor_pos, max_val)
        {
            state.start_move_anchor(tick);
            state.automation_curve_current = None;
            return Some(publish_velocity(VelocityAction::AutomationDragStart));
        }
        None
    }

    /// Curve 工具：开始曲线绘制
    fn handle_automation_curve_start(
        &self,
        state: &mut VelocityCanvasState,
        view: &lumino_gfx::automation::AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> Option<canvas::Action<Message>> {
        let tick = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0) as u32;
        let value = view
            .y_to_value(cursor_pos.y, max_val)
            .round()
            .clamp(0.0, max_val) as u16;
        state.start_curve_draw(tick, value);
        Some(publish_velocity(VelocityAction::AutomationDragStart))
    }

    /// 处理自动化拖拽移动
    pub(super) fn handle_automation_cursor_moved(
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
            AutomationDrag::MoveAnchor { old_tick } => self.handle_move_anchor_drag(
                state, view, target, max_val, track_idx, lane_idx, old_tick, cursor_pos,
            ),
            AutomationDrag::CurveDraw {
                start_tick,
                start_value,
            } => self.handle_curve_draw_drag(
                state,
                view,
                target,
                max_val,
                track_idx,
                start_tick,
                start_value,
                cursor_pos,
            ),
        }
    }

    /// 处理移动锚点拖拽
    fn handle_move_anchor_drag(
        &self,
        state: &mut VelocityCanvasState,
        view: lumino_gfx::automation::AutomationViewParams,
        target: lumino_note_core::AutomationTarget,
        max_val: f32,
        track_idx: u16,
        lane_idx: Option<usize>,
        old_tick: u32,
        cursor_pos: Point,
    ) -> Option<canvas::Action<Message>> {
        let lane_idx = lane_idx?;
        let new_tick_f = self.snap_tick(self.x_to_tick(cursor_pos.x)).max(0.0);
        let new_tick = new_tick_f as u32;
        let new_value = view
            .y_to_value(cursor_pos.y, max_val)
            .round()
            .clamp(0.0, max_val) as u16;
        state.automation_curve_current = Some((new_tick, new_value));

        if new_tick == old_tick {
            return self
                .handle_anchor_value_update(track_idx, lane_idx, old_tick, new_tick, new_value);
        }

        self.handle_anchor_position_move(
            state, track_idx, lane_idx, &target, old_tick, new_tick, new_value,
        )
    }

    /// 同 tick 仅更新 value
    fn handle_anchor_value_update(
        &self,
        track_idx: u16,
        lane_idx: usize,
        old_tick: u32,
        new_tick: u32,
        new_value: u16,
    ) -> Option<canvas::Action<Message>> {
        let edit = AutomationEdit::Move {
            track_idx,
            lane_idx,
            old_tick,
            // 非弯音场景：同 tick 唯一，按 tick 匹配即可
            old_value: None,
            new_tick,
            new_value,
        };
        Some(publish_velocity(VelocityAction::AutomationBatch(vec![
            edit,
        ])))
    }

    /// 移动到新 tick：先删除旧事件再添加新事件
    fn handle_anchor_position_move(
        &self,
        state: &mut VelocityCanvasState,
        track_idx: u16,
        lane_idx: usize,
        target: &lumino_note_core::AutomationTarget,
        old_tick: u32,
        new_tick: u32,
        new_value: u16,
    ) -> Option<canvas::Action<Message>> {
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
}
