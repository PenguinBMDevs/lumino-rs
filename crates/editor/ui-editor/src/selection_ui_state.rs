//! 框选交互的 UI 状态（T-30 slice 7）。
//!
//! 鼠标框选/弹簧动画/增量边界缓存同属一份状态：按下建立锚点、拖动更新
//! delta 边界、松手后动画收敛。集中到一个 bundle，`Editor` 只保留一个字段。

use std::cell::Cell;

use crate::SelectionBoxAnimState;

/// 框选交互状态。
#[derive(Debug, Default)]
pub(crate) struct SelectionUiState {
    /// 选择框边界缓存（raw 坐标），增量维护。
    pub(crate) bounds: Cell<Option<(f32, f32, u16, u16)>>,
    /// 上一帧框选矩形 raw 边界，用于计算增量 delta。
    pub(crate) cached_bounds: Cell<Option<(f32, f32, u16, u16)>>,
    /// 框选 X 向量化锚点（按下时 floor 到音符精度网格低边）。
    pub(crate) marquee_anchor: Option<f32>,
    /// 框选框弹簧动画显示状态。
    pub(crate) anim: Cell<Option<SelectionBoxAnimState>>,
    /// 待确认产物镜像已写回 owning tool 的累计偏移 `(dtick, dkey)`。
    pub(crate) pending_drag_applied: (f32, f32),
}
