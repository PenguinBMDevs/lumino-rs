//! 图形工具会话与 pending 编辑栈（T-30 slice 6）。
//!
//! shape edit / bake 放置 / area paint / slice / mould / round corner 以及
//! pending composed 的 undo/redo 栈都属于「形状交互会话」，生命周期一致：
//! 进入工具时创建、退出或提交时清空。集中到一个 bundle，`Editor` 只保留一个字段。

use std::collections::HashMap;

use lumino_editor_state::PendingComposed;

use crate::interaction::area_paint::AreaPaintState;
use crate::interaction::bake::BakePending;
use crate::interaction::round_corner::RoundCornerMode;
use crate::interaction::shape_bake_placement::BakePlacementSession;
use crate::interaction::shape_compose::{MouldKnife, MouldNotice};
use crate::interaction::shape_edit_ui::{ShapeEditState, ShapeEditTarget};
use crate::interaction::slice_cut::SliceCutState;

/// 图形工具会话与 pending 编辑栈。
#[derive(Debug, Default)]
pub(crate) struct ShapeToolState {
    /// 020 Custom edit 的手柄拖拽状态。
    pub(crate) edit: Option<ShapeEditState>,
    /// 020 显式编辑目标（None = 普通 ShapeSelect 选框）。
    pub(crate) edit_armed: Option<ShapeEditTarget>,
    /// 020 多选组编辑成员（`pending_composed` 下标）。
    pub(crate) group_edit: Option<Vec<usize>>,
    /// DRAW-049：Bake 放置确认态。
    pub(crate) bake_placement: Option<BakePlacementSession>,
    /// DRAW-041C：Bake 前组合会话的 redo 栈。
    pub(crate) pending_composed_redo: Vec<PendingComposed>,
    /// DRAW-041C：pending composed 手柄编辑的 undo 快照。
    pub(crate) pending_edit_undo: Vec<Vec<PendingComposed>>,
    /// DRAW-041C：pending composed 手柄编辑的 redo 快照。
    pub(crate) pending_edit_redo: Vec<Vec<PendingComposed>>,
    /// DRAW-032B：区域着色会话。
    pub(crate) area_paint: Option<AreaPaintState>,
    /// DRAW-025：切割布防。
    pub(crate) slice_cut: Option<SliceCutState>,
    /// DEBT-17：饼干模的刀。
    pub(crate) mould_knife: Option<MouldKnife>,
    /// DEBT-17：刀失效等一次性通知。
    pub(crate) mould_notice: Option<MouldNotice>,
    /// DRAW-044：角度柔化模式。
    pub(crate) round_corner: Option<RoundCornerMode>,
    /// DRAW-041B/032B：ShapeStore 模型列表是否展开。
    pub(crate) list_open: bool,
    /// DRAW-041：✓ 前 pending 矢量快照（按历史 group）。
    pub(crate) baked_pending: HashMap<u64, BakePending>,
    /// 待确认产物镜像拖动已写回的累计偏移 `(dtick, dkey)`。
    ///
    /// 从 `SelectionUiState` 迁入：它描述的是「形状会话的几何写回进度」，
    /// 不是框选 UI 状态。
    pub(crate) pending_drag_applied: (f32, f32),
}

impl ShapeToolState {
    /// 计算本次待确认镜像拖动的增量并推进已写回偏移；零增量返回 `None`。
    pub(crate) fn take_pending_drag_increment(
        &mut self,
        delta_tick: f32,
        delta_key: f32,
    ) -> Option<(f32, f32)> {
        let (applied_tick, applied_key) = self.pending_drag_applied;
        let inc = (delta_tick - applied_tick, delta_key - applied_key);
        if inc.0 == 0.0 && inc.1 == 0.0 {
            return None;
        }
        self.pending_drag_applied = (delta_tick, delta_key);
        Some(inc)
    }

    /// 一次待确认几何拖动结束后重置写回偏移。
    pub(crate) fn reset_pending_drag_applied(&mut self) {
        self.pending_drag_applied = (0.0, 0.0);
    }

    /// 记录一次 pending 编辑快照；undo 栈上限 64，新编辑清空 redo 与组合 redo。
    pub(crate) fn push_pending_edit_snapshot(&mut self, snapshot: Vec<PendingComposed>) {
        self.pending_edit_undo.push(snapshot);
        while self.pending_edit_undo.len() > 64 {
            self.pending_edit_undo.remove(0);
        }
        self.pending_edit_redo.clear();
        self.pending_composed_redo.clear();
    }

    /// 清空 pending 编辑的 undo/redo（不动组合 redo）。
    pub(crate) fn clear_pending_edit_stacks(&mut self) {
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
    }

    /// 清空全部 pending 栈（组合 redo + 编辑 undo/redo）。
    pub(crate) fn clear_pending_stacks(&mut self) {
        self.pending_composed_redo.clear();
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
    }

    /// 压入组合 redo，并清空本次组合产生的编辑栈。
    pub(crate) fn push_pending_composed_redo(&mut self, item: PendingComposed) {
        self.pending_composed_redo.push(item);
        self.clear_pending_edit_stacks();
    }
}
