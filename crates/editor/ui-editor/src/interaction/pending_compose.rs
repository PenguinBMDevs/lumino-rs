//! DRAW-041C：Bake 前形状组合会话（pending 层）
//!
//! 组合只改 pending：来源从 owning tool 摘除，结果进入
//! `EditorState::pending_composed`，✓ 才写文档音符。

mod edit;

use lumino_core::shape::{
    BetweenParams, BetweenRole, BetweenSet, MergeParams, ShapeGeometry, ShapeKind, ShapeModel,
    ShapeOp,
};
use lumino_editor_state::shape_ops::{
    cut_loops, cut_open_vertical, gate_merge, join, ring_of, split_notes_vertical, sync_groups,
    to_live,
};
use lumino_editor_state::{
    ComposedKind, ComposedPart, PendingComposed, PendingShapeRef, PendingSource,
};

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

        let mut results: Vec<ComposedPart> = match kind {
            ComposedKind::Live => {
                let Ok((model, _warnings)) = to_live(&models) else {
                    return false;
                };
                vec![ComposedPart {
                    model,
                    notes: notes.into_iter().flatten().collect(),
                }]
            }
            ComposedKind::Join => {
                let Some(joined) = join(&models, 1.0, 1.0) else {
                    return false;
                };
                vec![ComposedPart {
                    model: joined.model,
                    notes: notes.into_iter().flatten().collect(),
                }]
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
                vec![ComposedPart {
                    model,
                    notes: merged,
                }]
            }
            ComposedKind::Slice => {
                if models.len() != 1 {
                    return false;
                }
                let model = &models[0];
                let Some((x0, y0, x1, y1)) = model.geometry.bounds() else {
                    return false;
                };
                let cut = (x0 + x1) / 2.0;
                let Some((left, right)) = split_slice_parts(model, cut, y0, y1) else {
                    return false;
                };
                let (left_notes, right_notes) = split_notes_vertical(&notes[0], cut);
                vec![
                    ComposedPart {
                        model: polyline_model(left, track, model.k),
                        notes: left_notes,
                    },
                    ComposedPart {
                        model: polyline_model(right, track, model.k),
                        notes: right_notes,
                    },
                ]
            }
            ComposedKind::Between => {
                if models.len() != 2 {
                    return false;
                }
                let set = BetweenSet {
                    steps: 5,
                    ..BetweenSet::default()
                };
                let group_id = self.next_between_group_id();
                let mut first = models[0].clone();
                let mut last = models[1].clone();
                first.ops.push(ShapeOp::Between(BetweenParams {
                    id: group_id,
                    role: BetweenRole::First,
                    at: Some(0.0),
                    sig: None,
                    push: None,
                    vel: None,
                    set: Some(set),
                }));
                last.ops.push(ShapeOp::Between(BetweenParams {
                    id: group_id,
                    role: BetweenRole::Last,
                    at: Some(1.0),
                    sig: None,
                    push: None,
                    vel: None,
                    set: None,
                }));
                // 首/末只在 pending 会话内变化；sync_groups 负责生成步，
                // 手改过的 Step 会在纯函数里降级为 Key 并保留。
                let mut shapes = vec![first, last];
                for warning in sync_groups(&mut shapes) {
                    tracing::warn!(message = warning.message, "Between 组同步提示");
                }
                shapes
                    .into_iter()
                    .map(|model| {
                        let part_notes = self.shape_notes_of(&model, track);
                        ComposedPart {
                            model,
                            notes: part_notes,
                        }
                    })
                    .collect()
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
                vec![ComposedPart {
                    model,
                    notes: composed_notes,
                }]
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
                vec![ComposedPart {
                    model,
                    notes: composed_notes,
                }]
            }
            // 只在「进入编辑图形」时直接构造，不走这里
            ComposedKind::Single => return false,
        };
        for part in &mut results {
            part.model.id = 0;
        }

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
            results,
            sources,
        });
        self.pending_composed_redo.clear();
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
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
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
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
        self.pending_edit_undo.clear();
        self.pending_edit_redo.clear();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        true
    }

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

    /// 下一个可用的 Between 组 id：扫描已 Bake 模型与 pending 结果，避免跨组碰撞。
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

/// 两/多段闭环拍平成一条折线模型（Slice 的左右两片）
fn polyline_model(points: Vec<Vec<(f32, f32)>>, track: usize, k: f32) -> ShapeModel {
    let points: Vec<(f32, f32)> = points.into_iter().flatten().collect();
    let mut model = ShapeModel::new(0, track as u16, ShapeKind::Polyline, k);
    model.geometry = ShapeGeometry::from_points(points);
    model
}

/// 竖切两半的几何集合（与外层 Vec 分支同形）
type SliceHalves = (Vec<Vec<(f32, f32)>>, Vec<Vec<(f32, f32)>>);

/// 闭环优先走 `cut_loops`；开放折线（曲线/弧的展平）走竖线切段。
fn split_slice_parts(model: &ShapeModel, cut: f32, y0: f32, y1: f32) -> Option<SliceHalves> {
    if model.geometry.closed {
        cut_loops(&[ring_of(model)], (cut, y0 - 1.0), (cut, y1 + 1.0))
            .or_else(|| split_open_model(model, cut))
    } else {
        split_open_model(model, cut)
    }
}

/// 开放折线的竖切回退（`cut_loops` 只处理闭环）。
fn split_open_model(model: &ShapeModel, cut: f32) -> Option<SliceHalves> {
    let points = if model.geometry.closed || model.geometry.anchors.is_empty() {
        ring_of(model)
    } else {
        model
            .geometry
            .anchors
            .iter()
            .map(|anchor| anchor.pos)
            .collect()
    };
    cut_open_vertical(&points, cut).map(|(left, right)| (vec![left], vec![right]))
}
