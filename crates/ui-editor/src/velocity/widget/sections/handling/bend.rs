//! 弯音贝塞尔路径交互（Bend 模式 Curve 工具）
//!
//! 参考卷帘曲线工具（`interaction/line_tool`）：
//! - 空白按下：开始路径（按下起点 → 拖动 ghost → 松开完成两点路径）；
//! - 完整路径：点击曲线段插入锚点 / 拖动锚点移动 / 拖动控制柄弯曲 /
//!   双击中间锚点删除；
//! - **实时模式**（默认）：操作即时同步到 lane（`AutomationEdit` 系列）；
//! - **√× 模式**：操作只修改本地路径，√ 确认（`BendPathConfirm`）全量
//!   重建 lane，× 取消（`BendPathCancel`）丢弃。
//!
//! 命中测试与坐标转换见 [`super::bend_hit`]。

use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas;
use lumino_note_core::automation::{AutomationEdit, AutomationTarget};
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::bend_path::{BendAnchor, BendInteraction, HandleSide};
use super::super::super::state::VelocityCanvasState;
use super::bend_hit::{BendHit, bend_hit_test};
use super::publish_velocity;

/// √× 按钮尺寸（像素）
pub const BEND_BUTTON_SIZE: f32 = 22.0;
/// 按钮组与路径包围盒间距（像素）
pub const BUTTON_SPACING: f32 = 8.0;

/// √× 按钮矩形（面板局部坐标）
#[derive(Debug, Clone, Copy)]
pub struct BendButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// Bend 模式 Curve 工具按下入口（含 √× 按钮、双击、路径交互）
    pub(super) fn handle_bend_curve_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let (view, _target, max_val) = self.automation_view_params(bounds_size)?;

        // 1. √× 按钮命中（confirm 模式且路径完整）
        if self.editor.velocity_panel.bend_confirm_mode
            && let Some(rects) = super::super::super::drawing::bend::bend_button_screen_rects(
                &view,
                &state.bend_path,
                max_val,
                bounds_size,
            )
        {
            if rects.confirm.contains(cursor_pos) {
                let events = state
                    .bend_path
                    .anchors
                    .iter()
                    .map(|a| a.to_event())
                    .collect();
                state.bend_path.reset();
                return Some(publish_velocity(VelocityAction::BendPathConfirm(events)));
            }
            if rects.cancel.contains(cursor_pos) {
                state.bend_path.reset();
                return Some(publish_velocity(VelocityAction::BendPathCancel));
            }
        }

        // 2. 双击中间锚点 → 删除（端点不可删）
        if state.detect_double_click(cursor_pos) {
            if let Some(BendHit::Anchor { idx }) =
                bend_hit_test(&state.bend_path, &view, cursor_pos, max_val)
                && idx > 0
                && idx + 1 < state.bend_path.anchors.len()
            {
                let tick = state.bend_path.anchors[idx].pos.0.round() as u32;
                let (track_idx, lane_idx) = self.bend_lane_indices();
                state.bend_path.anchors.remove(idx);
                state.bend_path.recompute_auto_handles();
                state.bend_path.interaction = BendInteraction::None;
                // 实时模式：删除 lane 锚点
                if !self.editor.velocity_panel.bend_confirm_mode
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
            return None;
        }

        // 3. 完整路径：按 控制柄 > 锚点 > 曲线段 > 空白 分发
        if state.bend_path.is_complete() {
            match bend_hit_test(&state.bend_path, &view, cursor_pos, max_val) {
                Some(BendHit::Handle { idx, side }) => {
                    let anchor = &state.bend_path.anchors[idx];
                    state.bend_path.drag_handle_orig = match side {
                        HandleSide::In => anchor.in_handle,
                        HandleSide::Out => anchor.out_handle,
                    };
                    state.bend_path.interaction = BendInteraction::DraggingHandle { idx, side };
                    // 实时模式：push history（拖柄是一次编辑操作）
                    return self.bend_drag_start_live();
                }
                Some(BendHit::Anchor { idx }) => {
                    state.bend_path.drag_anchor_orig = state.bend_path.anchors[idx];
                    state.bend_path.interaction = BendInteraction::DraggingAnchor { idx };
                    return self.bend_drag_start_live();
                }
                Some(BendHit::Segment { segment }) => {
                    // 点击段：插入锚点（不吸附网格，位置 = 按下处）
                    let (tx, ty) = (
                        self.x_to_tick(cursor_pos.x).max(0.0),
                        view.y_to_value(cursor_pos.y, max_val).clamp(0.0, max_val),
                    );
                    let idx = segment + 1;
                    state
                        .bend_path
                        .anchors
                        .insert(idx, BendAnchor::new((tx, ty)));
                    state.bend_path.recompute_auto_handles();
                    // 实时模式：插入 lane 锚点
                    if !self.editor.velocity_panel.bend_confirm_mode {
                        return Some(publish_velocity(VelocityAction::AutomationEdit(
                            AutomationEdit::Add {
                                track_idx: self.editor.editor_state.data.current_track as u16,
                                target: AutomationTarget::PitchBend,
                                channel: 0,
                                tick: tx.round() as u32,
                                value: ty.round() as u16,
                                shape: lumino_note_core::SegmentShape::Curve { tension: 0 },
                            },
                        )));
                    }
                    return None;
                }
                None => {
                    // 空白处：开始新路径（替换当前路径，单路径模式）。
                    // 旧 lane 数据保留显示到本次绘制完成（released 时 commit 全量替换）。
                    let (tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
                    state.bend_path.reset();
                    state.bend_path.anchors.push(BendAnchor::new((tx, ty)));
                    state.bend_path.draw_start = (tx, ty);
                    state.bend_path.current = Some((tx, ty));
                    state.bend_path.interaction = BendInteraction::Drawing;
                    return None;
                }
            }
        }

        // 4. 绘制中（1 个锚点）：点击完成路径
        if state.bend_path.anchors.len() == 1 {
            let (tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
            state.bend_path.anchors.push(BendAnchor::new((tx, ty)));
            state.bend_path.recompute_auto_handles();
            state.bend_path.interaction = BendInteraction::None;
            state.bend_path.current = None;
            return self.commit_bend_path(state, max_val);
        }

        // 5. 无锚点：开始路径
        let (tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
        state.bend_path.anchors.push(BendAnchor::new((tx, ty)));
        state.bend_path.draw_start = (tx, ty);
        state.bend_path.current = Some((tx, ty));
        state.bend_path.interaction = BendInteraction::Drawing;
        None
    }

    /// Bend 模式 Curve 工具拖动更新
    pub(super) fn handle_bend_curve_moved(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let (view, _target, max_val) = self.automation_view_params(bounds_size)?;
        let raw = (
            self.x_to_tick(cursor_pos.x).max(0.0),
            view.y_to_value(cursor_pos.y, max_val).clamp(0.0, max_val),
        );
        state.bend_path.current = Some(raw);

        match state.bend_path.interaction {
            BendInteraction::None => None,
            BendInteraction::Drawing => None, // 只更新 ghost
            BendInteraction::DraggingAnchor { idx } => {
                let (tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
                if let Some(anchor) = state.bend_path.anchors.get_mut(idx) {
                    anchor.pos = (tx, ty);
                }
                state.bend_path.recompute_auto_handles();
                // 实时模式：移动 lane 锚点（旧 tick → 新位置）
                if !self.editor.velocity_panel.bend_confirm_mode {
                    let old_tick = state.bend_path.drag_anchor_orig.pos.0.round() as u32;
                    let (track_idx, lane_idx) = self.bend_lane_indices();
                    let lane_idx = lane_idx?;
                    let edit = AutomationEdit::Move {
                        track_idx,
                        lane_idx,
                        old_tick,
                        new_tick: tx.round() as u32,
                        new_value: ty.round() as u16,
                    };
                    return Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                        edit,
                    ])));
                }
                None
            }
            BendInteraction::DraggingHandle { idx, side } => {
                // 控制柄绝对位置直接跟随鼠标（按下时柄距锚点 < 命中半径，跳变可忽略）
                let h_abs = raw;
                if let Some(anchor) = state.bend_path.anchors.get_mut(idx) {
                    match side {
                        HandleSide::In => {
                            anchor.set_in_handle((h_abs.0 - anchor.pos.0, h_abs.1 - anchor.pos.1))
                        }
                        HandleSide::Out => {
                            anchor.set_out_handle((h_abs.0 - anchor.pos.0, h_abs.1 - anchor.pos.1))
                        }
                    }
                }
                // 实时模式：更新 lane 控制柄
                if !self.editor.velocity_panel.bend_confirm_mode {
                    let tick = state.bend_path.anchors[idx].pos.0.round() as u32;
                    let (track_idx, lane_idx) = self.bend_lane_indices();
                    let lane_idx = lane_idx?;
                    let anchor = state.bend_path.anchors[idx];
                    let edit = AutomationEdit::UpdateHandles {
                        track_idx,
                        lane_idx,
                        tick,
                        out_handle: anchor.out_handle,
                        in_handle: anchor.in_handle,
                    };
                    return Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                        edit,
                    ])));
                }
                None
            }
        }
    }

    /// Bend 模式 Curve 工具松开：完成两点绘制
    pub(super) fn handle_bend_curve_released(
        &self,
        state: &mut VelocityCanvasState,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        if state.bend_path.interaction != BendInteraction::Drawing {
            state.bend_path.interaction = BendInteraction::None;
            return None;
        }
        let (view, _target, max_val) = self.automation_view_params(bounds_size)?;
        let current = state
            .bend_path
            .current
            .unwrap_or(state.bend_path.draw_start);
        // 终点吸附（与按下一致）
        let (tx, ty) = self.bend_snap_pos_logical(&view, current, max_val);
        state.bend_path.anchors.push(BendAnchor::new((tx, ty)));
        state.bend_path.recompute_auto_handles();
        state.bend_path.interaction = BendInteraction::None;
        state.bend_path.current = None;
        self.commit_bend_path(state, max_val)
    }

    /// 实时模式：完成路径时写入 lane（清空 + 全量锚点）
    fn commit_bend_path(
        &self,
        state: &mut VelocityCanvasState,
        _max_val: f32,
    ) -> Option<canvas::Action<Message>> {
        if self.editor.velocity_panel.bend_confirm_mode {
            return None; // √× 模式：等待用户确认
        }
        let track_idx = self.editor.editor_state.data.current_track as u16;
        let target = AutomationTarget::PitchBend;
        let lane_idx = self
            .editor
            .editor_state
            .data
            .find_automation_lane(track_idx, &target);
        let mut edits: Vec<AutomationEdit> = Vec::with_capacity(state.bend_path.anchors.len() + 1);
        // 新路径替换旧路径（绘制完成 = 一次完整替换）
        if let Some(lane_idx) = lane_idx {
            edits.push(AutomationEdit::Clear {
                track_idx,
                lane_idx,
            });
        }
        for anchor in &state.bend_path.anchors {
            edits.push(AutomationEdit::Add {
                track_idx,
                target: target.clone(),
                channel: 0,
                tick: anchor.pos.0.round() as u32,
                value: anchor.pos.1.round() as u16,
                shape: lumino_note_core::SegmentShape::Curve { tension: 0 },
            });
        }
        Some(publish_velocity(VelocityAction::AutomationBatch(edits)))
    }

    /// 实时模式：拖拽开始（push history）
    fn bend_drag_start_live(&self) -> Option<canvas::Action<Message>> {
        if self.editor.velocity_panel.bend_confirm_mode {
            return None;
        }
        Some(publish_velocity(VelocityAction::AutomationDragStart))
    }

    /// 当前音轨 Bend lane 索引
    fn bend_lane_indices(&self) -> (u16, Option<usize>) {
        let track_idx = self.editor.editor_state.data.current_track as u16;
        let lane_idx = self
            .editor
            .editor_state
            .data
            .find_automation_lane(track_idx, &AutomationTarget::PitchBend);
        (track_idx, lane_idx)
    }

    /// 吸附逻辑位置：(tick 吸附网格, value 取整)
    fn bend_snap_pos(
        &self,
        view: &lumino_gfx::automation::AutomationViewParams,
        cursor_pos: Point,
        max_val: f32,
    ) -> (f32, f32) {
        let raw = (
            self.x_to_tick(cursor_pos.x).max(0.0),
            view.y_to_value(cursor_pos.y, max_val).clamp(0.0, max_val),
        );
        self.bend_snap_pos_logical(view, raw, max_val)
    }

    fn bend_snap_pos_logical(
        &self,
        _view: &lumino_gfx::automation::AutomationViewParams,
        raw: (f32, f32),
        max_val: f32,
    ) -> (f32, f32) {
        (
            self.snap_tick(raw.0).max(0.0),
            raw.1.round().clamp(0.0, max_val),
        )
    }
}
