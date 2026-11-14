//! pending 选区谓词（组合会话的前置条件）
//!
//! 从 `pending_compose.rs` 拆出（文件长度纪律）；只读判定，不改状态。

use lumino_editor_state::PendingShapeRef;

use crate::Editor;

impl Editor {
    /// 当前选中集是否全部是 pending 镜像（至少一个）；混合选区不走组合会话
    pub(crate) fn pending_selection_only(&self) -> bool {
        let mut any = false;
        for shape in self.editor_state.shape_select.selected_shapes() {
            if shape.pending.is_some() {
                any = true;
            } else {
                return false;
            }
        }
        any
    }

    /// 当前选中集是否恰好是一个 pending 矢量（形状/曲线），且不含已组合结果。
    ///
    /// 027/028 是单对象操作：镜像/组合结果仍走各自入口，不在这里重复组合。
    pub(crate) fn pending_single_selection(&self) -> bool {
        let mut count = 0usize;
        for shape in self.editor_state.shape_select.selected_shapes() {
            match shape.pending {
                Some(PendingShapeRef::Shape(_)) | Some(PendingShapeRef::CurvePath(_)) => {
                    count += 1;
                }
                _ => return false,
            }
        }
        count == 1
    }
}
