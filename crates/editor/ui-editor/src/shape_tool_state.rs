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
}
