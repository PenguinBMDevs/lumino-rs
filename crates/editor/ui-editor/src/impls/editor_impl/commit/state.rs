//! 编辑态判定与 pending 状态查询（不修改数据）。

use crate::{EditState, Editor};

impl Editor {
    /// 检查当前是否处于编辑状态（拦截 Undo/Redo/Save/Play/Export 用）
    ///
    /// 返回 `true` 当用户正在进行音符编辑（拖动/绘制/调整大小），
    /// 或有未提交的批量拖动/批量复制（pending_drag_state / pending_copy_drag_state），
    /// 或正在进行曲线路径编辑（锚点/控制柄拖动），
    /// 或正在落笔绘制/整体拖动画刷笔画。
    ///
    /// 注意：画刷的「待确认笔画」（已松手）**不算**编辑中——否则 Ctrl+Z 会被
    /// 这里拦截，用户无法用撤销回退笔画编辑（与曲线工具 pending 语义一致）。
    pub fn is_editing(&self) -> bool {
        self.transient.pending_drag.is_some()
            || self.transient.pending_copy.is_some()
            || self.editor_state.data.has_pending_commit()
            || self.editor_state.brush_tool.is_active()
            || self.editor_state.shape_select.is_dragging()
            || !matches!(
                self.editor_state.line_tool.interaction,
                lumino_editor_state::LineToolInteraction::None
                    | lumino_editor_state::LineToolInteraction::SelectedAnchor { .. }
            )
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::DraggingSelectionCopy { .. }
                    | EditState::PendingDrag { .. }
                    | EditState::Drawing { .. }
                    | EditState::ResizingStart { .. }
                    | EditState::ResizingEnd { .. }
                    | EditState::ResizingSelectionStart { .. }
                    | EditState::ResizingSelectionEnd { .. }
            )
    }

    /// 是否有未提交的批量拖动（pending commit 状态，含批量复制）
    pub fn has_pending_drag(&self) -> bool {
        self.transient.pending_drag.is_some()
            || self.transient.pending_copy.is_some()
            || self.editor_state.data.has_pending_commit()
    }

    /// 是否有尚未启动异步提交的待提交批量拖动（不含异步提交中的 pending commit）
    pub fn has_uncommitted_drag(&self) -> bool {
        self.transient.pending_drag.is_some()
    }

    /// 是否有尚未提交的批量复制（不含异步提交中的 pending commit）
    pub fn has_uncommitted_copy(&self) -> bool {
        self.transient.pending_copy.is_some()
    }

    /// 远端音符操作是否应延迟到本地编辑临界区结束后应用。
    ///
    /// 本地拖动/待提交/异步提交期间，`DragState.selected`（BitVec）与
    /// `note_index` 等引用当前轨的**索引**；若此时远端结构编辑（增/删/移/复）
    /// 落到同一轨，索引漂移会让本地拖动/提交引用错误音符（数据损坏）。
    /// 因此远端同轨结构编辑需入队延迟（`Root::deferred_remote_ops`），
    /// 待本地临界区结束（手势松手 / 提交完成）后按到达顺序补放。
    ///
    /// `affected_tracks`：远端操作涉及的音轨集合（含 source/target 轨）。
    pub fn should_defer_remote_note_ops(&self, affected_tracks: &[usize]) -> bool {
        self.is_editing() && affected_tracks.contains(&self.editor_state.data.current_track)
    }

    /// 丢弃未提交的批量拖动/批量复制（不含异步提交中的 pending commit）
    ///
    /// 图片转 MIDI √ 写入后调用：写入改变了 document 音符数量与顺序，
    /// 残留的 `pending_drag_state.selected` / `pending_copy_drag_state.selected`
    /// 是写入前的全局索引，继续保留会导致后续 resize/拖动按旧索引取位、
    /// 误伤周围音符（连带改变长度）。
    pub fn clear_pending_drag(&mut self) {
        self.transient.pending_drag = None;
        self.transient.pending_copy = None;
    }
}
