//! DRAW-041C：Bake 前形状组合会话（pending 层）
//!
//! 组合只改 pending：来源从 owning tool 摘除，结果进入
//! `EditorState::pending_composed`，✓ 才写文档音符。

use lumino_core::shape::{MergeParams, ShapeOp};
use lumino_editor_state::shape_ops::{gate_merge, join, to_live};
use lumino_editor_state::{ComposedKind, PendingComposed, PendingShapeRef, PendingSource};

use crate::Editor;
use crate::interaction::shape_bridge::model_from_source;
use crate::interaction::shape_compose::CopyAlongSpec;

impl Editor {
    /// 当前选中集是否全部是 pending 镜像（至少一个）；混合选区不走组合会话
    pub(super) fn pending_selection_only(&self) -> bool {
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
    pub(super) fn pending_single_selection(&self) -> bool {
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

    /// 把选中的 pending 形状/路径组合成一个 pending composed 结果。
    pub(super) fn compose_pending(&mut self, kind: ComposedKind) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        self.compose_pending_with(kind, copy_spec)
    }

    /// 027 pending 入口：按显式 tick 步长/总组数组合。
    pub(super) fn compose_pending_copy_along(&mut self, spec: CopyAlongSpec) -> bool {
        self.compose_pending_with(ComposedKind::CopyAlong, spec)
    }

    fn compose_pending_with(&mut self, kind: ComposedKind, copy_spec: CopyAlongSpec) -> bool {
        let refs: Vec<PendingShapeRef> = self
            .editor_state
            .shape_select
            .selected_shapes()
            .filter_map(|shape| shape.pending)
            .collect();
        if refs.is_empty() {
            return false;
        }
        let track = self.editor_state.data.current_track;
        let mut sources = Vec::with_capacity(refs.len());
        let mut models = Vec::with_capacity(refs.len());
        let mut notes = Vec::with_capacity(refs.len());
        for reference in &refs {
            let Some(source) = self.pending_source(*reference) else {
                return false;
            };
            let model = model_from_source(&source, track, 1.0);
            let model_notes = self.shape_notes_of(&model, track);
            let pending_source = match reference {
                PendingShapeRef::Shape(i) => {
                    PendingSource::Shape(self.editor_state.shape_tool.shapes[*i].clone())
                }
                PendingShapeRef::CurvePath(i) => {
                    PendingSource::CurvePath(self.editor_state.line_tool.paths[*i].clone())
                }
                PendingShapeRef::BrushStroke(_) | PendingShapeRef::Composed(_) => return false,
            };
            sources.push(pending_source);
            models.push(model);
            notes.push(model_notes);
        }

        let (mut model, composed_notes) = match kind {
            ComposedKind::Live => {
                let Ok((model, _warnings)) = to_live(&models) else {
                    return false;
                };
                (model, notes.into_iter().flatten().collect::<Vec<_>>())
            }
            ComposedKind::Join => {
                let Some(joined) = join(&models, 1.0, 1.0) else {
                    return false;
                };
                (
                    joined.model,
                    notes.into_iter().flatten().collect::<Vec<_>>(),
                )
            }
            ComposedKind::Merge => {
                if models.len() != 2 {
                    return false;
                }
                let params = MergeParams {
                    stay: 0,
                    from_left: None,
                    rest_slides: false,
                };
                let Some(outcome) = gate_merge(&notes[0], &notes[1], &params) else {
                    return false;
                };
                let mut merged = notes[0].clone();
                for note in &notes[1] {
                    let shift =
                        outcome.by_key.get(&note.key).copied().unwrap_or(0) + outcome.shift_all;
                    let mut shifted = *note;
                    shifted.tick = (shifted.tick + shift as f32).max(0.0);
                    merged.push(shifted);
                }
                let mut model = models[0].clone();
                model.ops.push(ShapeOp::Merge(params));
                (model, merged)
            }
            ComposedKind::CopyAlong => {
                if models.len() != 1 {
                    return false;
                }
                let Some(mut model) = models.pop() else {
                    return false;
                };
                let Some(params) = Self::default_copy_along_params(&model, copy_spec) else {
                    return false;
                };
                model.ops.push(ShapeOp::CopyAlong(params));
                let composed_notes = self.shape_notes_of(&model, track);
                (model, composed_notes)
            }
            ComposedKind::Velocity => {
                if models.len() != 1 {
                    return false;
                }
                let Some(mut model) = models.pop() else {
                    return false;
                };
                model
                    .ops
                    .retain(|op| !matches!(op, ShapeOp::VelocityEnv(_)));
                model
                    .ops
                    .push(ShapeOp::VelocityEnv(Self::rising_velocity_env()));
                let composed_notes = self.shape_notes_of(&model, track);
                (model, composed_notes)
            }
        };
        model.id = 0;

        let mut shape_indices = Vec::new();
        let mut curve_indices = Vec::new();
        for reference in &refs {
            match reference {
                PendingShapeRef::Shape(i) => shape_indices.push(*i),
                PendingShapeRef::CurvePath(i) => curve_indices.push(*i),
                _ => return false,
            }
        }
        shape_indices.sort_unstable_by(|a, b| b.cmp(a));
        curve_indices.sort_unstable_by(|a, b| b.cmp(a));
        for index in shape_indices {
            self.editor_state.shape_tool.shapes.remove(index);
        }
        for index in curve_indices {
            self.editor_state.line_tool.paths.remove(index);
        }

        let composed_index = self.editor_state.pending_composed.len();
        self.editor_state.pending_composed.push(PendingComposed {
            kind,
            model,
            notes: composed_notes,
            sources,
        });
        self.pending_composed_redo.clear();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        if let Some(id) = self
            .editor_state
            .shape_select
            .pending_mirror(PendingShapeRef::Composed(composed_index))
        {
            self.editor_state.shape_select.select_only(id);
        }
        self.grid_cache.clear();
        true
    }

    /// 撤销上一次 Bake 前组合（回到 pending 矢量）
    pub(crate) fn undo_pending_compose(&mut self) -> bool {
        let Some(item) = self.editor_state.pending_composed.pop() else {
            return false;
        };
        for source in &item.sources {
            self.restore_pending_source(source.clone());
        }
        self.pending_composed_redo.push(item);
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }

    /// 重做上一次 Bake 前组合
    pub(crate) fn redo_pending_compose(&mut self) -> bool {
        let Some(item) = self.pending_composed_redo.pop() else {
            return false;
        };
        for source in &item.sources {
            self.remove_pending_source(source);
        }
        self.editor_state.pending_composed.push(item);
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }

    fn restore_pending_source(&mut self, source: PendingSource) {
        match source {
            PendingSource::Shape(shape) => self.editor_state.shape_tool.shapes.push(shape),
            PendingSource::CurvePath(path) => self.editor_state.line_tool.paths.push(path),
        }
    }

    fn remove_pending_source(&mut self, source: &PendingSource) {
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
