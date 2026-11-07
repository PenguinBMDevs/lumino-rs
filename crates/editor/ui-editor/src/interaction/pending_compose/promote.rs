//! DRAW-041C：020 提升为单支组合 + pending 来源恢复/移除。
//!
//! 从 `pending_compose.rs` 拆出（文件长度纪律）。
use lumino_core::shape::ShapeOp;
use lumino_editor_state::{ComposedKind, PendingComposed, PendingShapeRef, PendingSource};

use crate::Editor;

impl Editor {
    /// 020：把当前选中的单个待确认形状/曲线提升为可编辑的单支组合。
    ///
    /// 参数化实例（`ShapeInstance` 只有 kind + rect）表达不了旋转，所以进入
    /// 「编辑图形」前统一提升为模型；`sources` 保留原实例，
    /// `Ctrl+Z`（撤销组合）能还原回参数化形状。
    pub(crate) fn promote_pending_for_edit(&mut self) -> bool {
        let reference = {
            let mut selected = self.editor_state.shape_select.selected_shapes();
            let Some(shape) = selected.next() else {
                return false;
            };
            if selected.next().is_some() {
                return false;
            }
            match shape.pending {
                Some(reference @ (PendingShapeRef::Shape(_) | PendingShapeRef::CurvePath(_))) => {
                    reference
                }
                _ => return false,
            }
        };
        let track = self.editor_state.data.current_track;
        let Some(source) = self.pending_source(reference) else {
            return false;
        };
        let mut model = crate::interaction::shape_bridge::model_from_source(&source, track, 1.0);
        // 待确认 Mask 标记要跟着模型走，否则提升后预览会莫名消失
        let pending_source = match reference {
            PendingShapeRef::Shape(index) => {
                let Some(shape) = self.editor_state.shape_tool.shapes.get(index).cloned() else {
                    return false;
                };
                if let Some(params) = shape.mask.clone() {
                    model.ops.push(ShapeOp::Mask(params));
                }
                PendingSource::Shape(shape)
            }
            PendingShapeRef::CurvePath(index) => {
                let Some(path) = self.editor_state.line_tool.paths.get(index).cloned() else {
                    return false;
                };
                PendingSource::CurvePath(path)
            }
            PendingShapeRef::Composed(_) | PendingShapeRef::BrushStroke(_) => return false,
        };
        let notes = self.shape_notes_of(&model, track);
        let index = self.editor_state.pending_composed.len();
        self.editor_state
            .pending_composed
            .push(PendingComposed::single(
                ComposedKind::Single,
                model,
                notes,
                vec![pending_source],
            ));
        match reference {
            PendingShapeRef::Shape(i) => {
                self.editor_state.shape_tool.shapes.remove(i);
            }
            PendingShapeRef::CurvePath(i) => {
                self.editor_state.line_tool.paths.remove(i);
            }
            _ => {}
        }
        self.pending_composed_redo.clear();
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        if let Some(id) = self
            .editor_state
            .shape_select
            .pending_mirror(PendingShapeRef::Composed(index))
        {
            self.editor_state.shape_select.select_only(id);
        }
        self.grid_cache.clear();
        true
    }

    pub(super) fn restore_pending_source(&mut self, source: PendingSource) {
        match source {
            PendingSource::Shape(shape) => self.editor_state.shape_tool.shapes.push(shape),
            PendingSource::CurvePath(path) => self.editor_state.line_tool.paths.push(path),
        }
    }

    pub(super) fn remove_pending_source(&mut self, source: &PendingSource) {
        match source {
            PendingSource::Shape(shape) => {
                if let Some(index) = self
                    .editor_state
                    .shape_tool
                    .shapes
                    .iter()
                    .position(|item| item == shape)
                {
                    self.editor_state.shape_tool.shapes.remove(index);
                }
            }
            PendingSource::CurvePath(path) => {
                if let Some(index) = self
                    .editor_state
                    .line_tool
                    .paths
                    .iter()
                    .position(|item| item == path)
                {
                    self.editor_state.line_tool.paths.remove(index);
                }
            }
        }
    }
}
