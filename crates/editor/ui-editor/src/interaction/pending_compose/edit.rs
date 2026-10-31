//! DRAW-041C 阶段 3：pending composed 手柄编辑的 undo/redo。
//!
//! 只动 `EditorState::pending_composed`，不进入文档历史；快照在
//! `shape_edit_ui::finish_shape_edit` 提交时压栈。

use lumino_core::shape::{ShapeModel, ShapeOp};

use crate::Editor;

impl Editor {
    /// 下一个可用的 Between 组 id：扫描已 Bake 模型与 pending 结果，避免跨组碰撞。
    pub(crate) fn next_between_group_id(&self) -> u64 {
        let stored = self
            .editor_state
            .shape_store
            .iter()
            .filter_map(|entry| between_group_id(&entry.model));
        let pending = self
            .editor_state
            .pending_composed
            .iter()
            .flat_map(|item| item.results.iter())
            .filter_map(|part| between_group_id(&part.model));
        stored.chain(pending).max().unwrap_or(0) + 1
    }

    /// 撤销一次 pending composed 手柄编辑（不进入文档历史）
    pub(crate) fn undo_pending_edit(&mut self) -> bool {
        let Some(previous) = self.pending_edit_undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.editor_state.pending_composed, previous);
        self.pending_edit_redo.push(current);
        self.editor_state.shape_select.remove_pending_all();
        self.shape_edit_armed = None;
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }

    /// 重做一次 pending composed 手柄编辑
    pub(crate) fn redo_pending_edit(&mut self) -> bool {
        let Some(next) = self.pending_edit_redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.editor_state.pending_composed, next);
        self.pending_edit_undo.push(current);
        self.editor_state.shape_select.remove_pending_all();
        self.shape_edit_armed = None;
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }
}

fn between_group_id(model: &ShapeModel) -> Option<u64> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Between(params) => Some(params.id),
        _ => None,
    })
}
