//! 020「编辑图形」松手提交 / 取消 / 单支 pending 音符重生成。
//!
//! 从 `shape_edit_ui.rs` 拆出，保持单文件 <400 行。

use lumino_core::shape::{ShapeModel, ShapeOp};
use lumino_editor_state::ShapeNote;

use crate::Editor;
use crate::interaction::shape_bridge::source_from_model;

use super::ShapeEditTarget;

impl Editor {
    /// 松手提交；无实际变化或失败时恢复原几何
    pub(crate) fn finish_shape_edit(&mut self) -> bool {
        let Some(state) = self.shape_edit.take() else {
            return false;
        };
        let changed = match state.target {
            ShapeEditTarget::Group => self.editor_state.pending_composed != state.original_composed,
            _ => state.current != state.original,
        };
        match state.target {
            ShapeEditTarget::Drawn { drawn_id, model_id } => {
                if !changed {
                    let source = source_from_model(&state.original);
                    self.editor_state.shape_select.set_source(drawn_id, source);
                    return false;
                }
                let ok = self.edit_shape(model_id, state.current);
                if !ok {
                    let source = source_from_model(&state.original);
                    self.editor_state.shape_select.set_source(drawn_id, source);
                }
                self.grid_cache.clear();
                ok
            }
            ShapeEditTarget::PendingComposed { index } => {
                if !changed {
                    self.restore_pending_part(index, &state.original);
                    self.refresh_pending_mirror_geometry();
                    self.grid_cache.clear();
                    return false;
                }
                let notes = self.part_notes_for_edit(&state.current, state.current.track as usize);
                if let Some(part) = self
                    .editor_state
                    .pending_composed
                    .get_mut(index)
                    .and_then(|composed| composed.results.first_mut())
                {
                    part.model = state.current;
                    part.notes = notes;
                }
                self.pending_edit_undo.push(state.original_composed);
                while self.pending_edit_undo.len() > 64 {
                    self.pending_edit_undo.remove(0);
                }
                self.pending_edit_redo.clear();
                self.pending_composed_redo.clear();
                self.refresh_pending_mirror_geometry();
                self.grid_cache.clear();
                true
            }
            ShapeEditTarget::Group => {
                if !changed {
                    self.restore_group_snapshot(state.original_composed);
                    self.grid_cache.clear();
                    return false;
                }
                // 一次组拖动 = 一次 undo：快照包含全部成员模型与音符
                self.pending_edit_undo.push(state.original_composed);
                while self.pending_edit_undo.len() > 64 {
                    self.pending_edit_undo.remove(0);
                }
                self.pending_edit_redo.clear();
                self.pending_composed_redo.clear();
                self.refresh_pending_mirror_geometry();
                self.grid_cache.clear();
                true
            }
        }
    }

    /// 取消拖拽并恢复原几何（Esc / 退出编辑图形）
    pub(crate) fn cancel_shape_edit(&mut self) {
        let Some(state) = self.shape_edit.take() else {
            return;
        };
        match state.target {
            ShapeEditTarget::Drawn { drawn_id, .. } => {
                let source = source_from_model(&state.original);
                self.editor_state.shape_select.set_source(drawn_id, source);
            }
            ShapeEditTarget::PendingComposed { index } => {
                self.restore_pending_part(index, &state.original);
                self.refresh_pending_mirror_geometry();
            }
            ShapeEditTarget::Group => {
                self.restore_group_snapshot(state.original_composed);
            }
        }
        self.grid_cache.clear();
    }

    fn restore_pending_part(&mut self, index: usize, model: &ShapeModel) {
        let notes = self.part_notes_for_edit(model, model.track as usize);
        if let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        {
            part.model = model.clone();
            part.notes = notes;
        }
    }

    /// Live 的结果音符来自各 `from` 的原始音符；直接按展平几何重生成会得到
    /// 一条错误的连接线。其余单支模型可按当前 model 重生成。
    pub(super) fn part_notes_for_edit(&self, model: &ShapeModel, track: usize) -> Vec<ShapeNote> {
        if let Some(ShapeOp::Live(params)) =
            model.ops.iter().find(|op| matches!(op, ShapeOp::Live(_)))
            && !params.from.is_empty()
        {
            let mut notes = Vec::new();
            for from in &params.from {
                notes.extend(self.shape_notes_of(from, from.track as usize));
            }
            notes
        } else {
            self.shape_notes_of(model, track)
        }
    }
}
