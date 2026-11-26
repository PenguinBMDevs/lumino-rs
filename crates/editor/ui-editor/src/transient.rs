//! `Editor` 的瞬时编辑状态（T-30 slice 2）。
//!
//! 这些字段共同描述「一次未提交的编辑」：远程协作光标、待提交拖拽、待提交复制。
//! 收敛到一个 bundle 后，Editor 的顶层字段减少，状态边界也更清楚。

use std::collections::HashMap;

use lumino_editor_state::DragState;
use lumino_ui_core::geom::Point;

/// 协作 / 拖拽的瞬时状态。
#[derive(Debug, Default)]
pub struct EditorTransient {
    /// 协作远程用户：用户 ID -> (位置, 颜色, 用户名)。
    pub remote_cursors: HashMap<String, (Point, String, String)>,
    /// 待提交的拖拽（ghost 偏移）。
    pub(crate) pending_drag: Option<DragState>,
    /// 待提交的复制拖拽。
    pub(crate) pending_copy: Option<DragState>,
}
