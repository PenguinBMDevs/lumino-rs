//! 弯音贝塞尔路径交互（Bend 模式 Curve 工具）
//!
//! 交互模型（参考卷帘曲线工具 `interaction/line_tool`，但为连续追加模式）：
//! - 空白处点击：**追加一个锚点**（从第一个锚点起，每点击一次追加一个，
//!   形成线段，立即更改弯音）；
//! - 点击锚点：**选中**（高亮显示），可拖动移动；
//! - 拖动控制柄：弯曲对应贝塞尔段（选中锚点的柄高亮）；
//! - 点击曲线段：插入新锚点（自由定位，插入后自动重算相邻段柄）；
//! - 双击中间锚点：删除。
//!
//! 所有操作**实时生效**：即时同步到 lane（`AutomationEdit` 系列），
//! 画完立即听到弯音效果。
//!
//! 命中测试与坐标转换见 [`super::bend_hit`]。

use iced_core::{Point, Size};
use iced_widget::canvas;
use lumino_note_core::automation::{AutomationEdit, AutomationTarget};
use lumino_ui_core::Message;
use lumino_ui_core::message::VelocityAction;

use super::super::super::bend_path::{BendAnchor, BendInteraction, HandleSide};
use super::super::super::state::VelocityCanvasState;
use super::bend_hit::{BendHit, bend_hit_test};
use super::publish_velocity;

/// 点击 vs 拖动判定阈值（像素）：移动距离低于此值视为纯点击（选中），
/// 不改变锚点高度；超过才进入拖动（高度改变必须手动拖动）
const CLICK_DRAG_THRESHOLD_PX: f32 = 4.0;

impl<'a> super::super::super::VelocityCanvas<'a> {
    /// Bend 模式 Curve 工具按下入口（双击删除 / 命中分发 / 空白追加）
    pub(super) fn handle_bend_curve_pressed(
        &self,
        state: &mut VelocityCanvasState,
        cursor_pos: Point,
        bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        let (view, _target, max_val) = self.automation_view_params(bounds_size)?;

        // 手势开始统一重置交互：清除任何残留（如拖拽中鼠标移出面板，
        // iced 不派发 released 导致的 DraggingAnchor/DraggingHandle 残留）。
        // 只有显式命中 Handle/Anchor 才进入拖拽；创建路径（段插入/空白追加）
        // 永不进入拖拽状态 —— 锚点只能点击创建，创建后不跟随鼠标。
        state.bend_path.interaction = BendInteraction::None;

        // 双击中间锚点 → 删除（端点不可删）
        if state.detect_double_click(cursor_pos) {
            if let Some(BendHit::Anchor { idx }) =
                bend_hit_test(&state.bend_path, &view, cursor_pos, max_val)
                && idx > 0
                && idx + 1 < state.bend_path.anchors.len()
            {
                let tick = state.bend_path.anchors[idx].pos.0.round() as u32;
                let (track_idx, lane_idx) = self.bend_lane_indices();
                // 跳变对（同 tick 两个锚点）整体删除：lane 侧 Delete 按
                // tick 移除全部同 tick 事件，本地保持一致（竖直段整体消失）
                state
                    .bend_path
                    .anchors
                    .retain(|a| a.pos.0.round() as u32 != tick);
                state.bend_path.recompute_auto_handles();
                state.bend_path.selected = None;
                state.bend_path.interaction = BendInteraction::None;
                if let Some(lane_idx) = lane_idx {
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

        // 命中分发：控制柄 > 锚点 > 曲线段 > 空白（追加锚点）
        match bend_hit_test(&state.bend_path, &view, cursor_pos, max_val) {
            Some(BendHit::Handle { idx, side }) => {
                let anchor = &state.bend_path.anchors[idx];
                state.bend_path.drag_handle_orig = match side {
                    HandleSide::In => anchor.in_handle,
                    HandleSide::Out => anchor.out_handle,
                };
                state.bend_path.interaction = BendInteraction::DraggingHandle { idx, side };
                // push history（拖柄是一次编辑操作）
                Some(publish_velocity(VelocityAction::AutomationDragStart))
            }
            Some(BendHit::Anchor { idx }) => {
                state.bend_path.drag_anchor_orig = state.bend_path.anchors[idx];
                state.bend_path.selected = Some(idx);
                state.bend_path.interaction = BendInteraction::DraggingAnchor { idx };
                // 记录按下屏幕位置：移动距离低于阈值视为纯点击（选中），
                // 不改变锚点高度——高度改变必须手动拖动
                state.bend_path.drag_press_screen = Some((cursor_pos.x, cursor_pos.y));
                Some(publish_velocity(VelocityAction::AutomationDragStart))
            }
            Some(BendHit::Segment { segment }) => {
                // 点击段：插入锚点（不吸附网格，位置 = 按下处）
                let (tx, ty) = (
                    self.x_to_tick(cursor_pos.x).max(0.0),
                    view.y_to_value(cursor_pos.y, max_val).clamp(0.0, max_val),
                );
                // 同一 tick 最多 2 个锚点（直角弯音跳变对）：已满则拒绝
                if Self::same_tick_anchor_count(&state.bend_path, tx) >= 2 {
                    return None;
                }
                // 插入位置必须在段端点 tick 区间内：竖直段（同 tick 跳变对）
                // 没有中间 tick，点击其上的插入会乱序（raw tick 必然越界）
                let a_tick = state.bend_path.anchors[segment].pos.0;
                let b_tick = state.bend_path.anchors[segment + 1].pos.0;
                if tx < a_tick || tx > b_tick {
                    return None;
                }
                let idx = segment + 1;
                state
                    .bend_path
                    .anchors
                    .insert(idx, BendAnchor::new((tx, ty)));
                state.bend_path.recompute_auto_handles();
                state.bend_path.selected = Some(idx);
                // 创建路径不进入拖拽状态（开头已重置，此处显式声明语义）
                state.bend_path.interaction = BendInteraction::None;
                // 实时：插入 lane 锚点
                Some(publish_velocity(VelocityAction::AutomationEdit(
                    AutomationEdit::Add {
                        track_idx: self.editor.editor_state.data.current_track as u16,
                        target: AutomationTarget::PitchBend,
                        channel: 0,
                        tick: tx.round() as u32,
                        value: ty.round() as u16,
                        shape: lumino_note_core::SegmentShape::Curve { tension: 0 },
                    },
                )))
            }
            None => {
                // 空白处：追加锚点（从第一个锚点起连续追加，立即生效）
                let (tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
                // 同一 tick 最多 2 个锚点（直角弯音跳变对）：已满则拒绝。
                // 注意：允许与已有锚点完全重合（同 tick 同 value）——这是
                // 跳变对的初始状态：用户点击同一高度创建第二个锚点（与
                // 第一个重叠显示），随后拖动它上行/下行分离形成突变。
                // 屏幕重合（<=HIT_RADIUS）由 hit test 的 Anchor 命中处理
                // （选中+拖动），此处处理网格吸附后的重合创建。
                if Self::same_tick_anchor_count(&state.bend_path, tx) >= 2 {
                    return None;
                }
                // 按 tick 有序插入（不能 push）：吸附后的 tick 可能小于
                // 已有锚点（点击偏左落入前一个网格），push 会造成锚点
                // 乱序——渲染（B→C 段倒退）与采样全部错误，连线视觉上
                // "绕过中间锚点"。有序插入保持 tick 升序不变式；
                // 用 `<=` 使同 tick 新锚点排在同 tick 已有锚点之后
                // （保持创建顺序 = 播放跳变方向：先建的高值在下行跳变中
                // 在前）。
                let insert_idx = state.bend_path.anchors.partition_point(|a| a.pos.0 <= tx);
                state
                    .bend_path
                    .anchors
                    .insert(insert_idx, BendAnchor::new((tx, ty)));
                state.bend_path.recompute_auto_handles();
                state.bend_path.selected = Some(insert_idx);
                state.bend_path.interaction = BendInteraction::None;
                Some(publish_velocity(VelocityAction::AutomationEdit(
                    AutomationEdit::Add {
                        track_idx: self.editor.editor_state.data.current_track as u16,
                        target: AutomationTarget::PitchBend,
                        channel: 0,
                        tick: tx.round() as u32,
                        value: ty.round() as u16,
                        shape: lumino_note_core::SegmentShape::Curve { tension: 0 },
                    },
                )))
            }
        }
    }

    /// Bend 模式 Curve 工具拖动更新（拖锚点移动 / 拖控制柄弯曲）
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

        match state.bend_path.interaction {
            BendInteraction::None => None,
            BendInteraction::DraggingAnchor { idx } => {
                // 拖动阈值：移动距离低于阈值 = 纯点击（选中），不改变高度。
                // 锚点高度改变必须手动拖动（超过阈值）
                if let Some((px, py)) = state.bend_path.drag_press_screen
                    && (cursor_pos.x - px).hypot(cursor_pos.y - py) < CLICK_DRAG_THRESHOLD_PX
                {
                    return None;
                }
                // 锚点 tick 锁定：只能上下调整 value，不能左右拖动
                // （锚点时间位置由创建时的点击决定）
                let (_tx, ty) = self.bend_snap_pos(&view, cursor_pos, max_val);
                if let Some(anchor) = state.bend_path.anchors.get_mut(idx) {
                    anchor.pos = (anchor.pos.0, ty);
                }
                state.bend_path.recompute_auto_handles();
                // 实时：更新 lane 锚点 value（tick 不变）
                let old_tick = state.bend_path.drag_anchor_orig.pos.0.round() as u32;
                let (track_idx, lane_idx) = self.bend_lane_indices();
                let lane_idx = lane_idx?;
                let edit = AutomationEdit::Move {
                    track_idx,
                    lane_idx,
                    old_tick,
                    // 精确匹配：跳变对（同 tick 两事件）按原值定位目标
                    old_value: Some(state.bend_path.drag_anchor_orig.pos.1.round() as u16),
                    new_tick: old_tick,
                    new_value: ty.round() as u16,
                };
                Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                    edit,
                ])))
            }
            BendInteraction::DraggingHandle { idx, side } => {
                // 控制柄绝对位置直接跟随鼠标（按下时柄距锚点 < 命中半径，跳变可忽略）
                let h_abs = raw;
                // 先复制相邻锚点位置（避免与可变借用冲突）
                let prev_tick = (idx > 0).then(|| state.bend_path.anchors[idx - 1].pos.0);
                let next_tick = (idx + 1 < state.bend_path.anchors.len())
                    .then(|| state.bend_path.anchors[idx + 1].pos.0);
                if let Some(anchor) = state.bend_path.anchors.get_mut(idx) {
                    let offset = (h_abs.0 - anchor.pos.0, h_abs.1 - anchor.pos.1);
                    match side {
                        HandleSide::In => {
                            // 入向柄钳制：不能越过自身（>0 无效）与上一锚点
                            let mut offset = offset;
                            if let Some(prev_tick) = prev_tick {
                                let min_x = (prev_tick - anchor.pos.0).min(0.0);
                                offset.0 = offset.0.clamp(min_x, 0.0);
                            }
                            anchor.set_in_handle(offset);
                        }
                        HandleSide::Out => {
                            // 出向柄钳制：不能越过自身（<0 无效）与下一锚点
                            let mut offset = offset;
                            if let Some(next_tick) = next_tick {
                                let max_x = (next_tick - anchor.pos.0).max(0.0);
                                offset.0 = offset.0.clamp(0.0, max_x);
                            }
                            anchor.set_out_handle(offset);
                        }
                    }
                }
                // 实时：更新 lane 控制柄
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
                Some(publish_velocity(VelocityAction::AutomationBatch(vec![
                    edit,
                ])))
            }
        }
    }

    /// Bend 模式 Curve 工具松开：结束交互（选中状态保持）
    pub(super) fn handle_bend_curve_released(
        &self,
        state: &mut VelocityCanvasState,
        _bounds_size: Size,
    ) -> Option<canvas::Action<Message>> {
        state.bend_path.interaction = BendInteraction::None;
        state.bend_path.drag_press_screen = None;
        None
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

    /// 与指定 tick 相同的锚点数量。
    ///
    /// 同一 tick 最多允许 2 个锚点（直角弯音跳变对：值瞬间从旧值跳到
    /// 新值）。超过 2 个会形成无法表达的重复跳变，禁止创建。
    fn same_tick_anchor_count(
        state: &crate::velocity::widget::bend_path::BendPathState,
        tick: f32,
    ) -> usize {
        state.anchors.iter().filter(|a| a.pos.0 == tick).count()
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
        (
            self.snap_tick(raw.0).max(0.0),
            raw.1.round().clamp(0.0, max_val),
        )
    }
}
