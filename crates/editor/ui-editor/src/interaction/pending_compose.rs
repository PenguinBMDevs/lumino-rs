//! DRAW-041C：Bake 前形状组合会话（pending 层）
//!
//! 组合只改 pending：来源从 owning tool 摘除，结果进入
//! `EditorState::pending_composed`，✓ 才写文档音符。

mod edit;
mod promote;
mod slice;

use lumino_core::shape::{BetweenParams, BetweenRole, BetweenSet, MergeParams, ShapeOp};
use lumino_editor_state::shape_ops::{
    CutKind, CutMark, CutRef, KnifeKind, extend_line, gate_merge, join, mould_pieces, ring_of,
    split_notes_line, sync_groups, to_live,
};

use self::slice::{polyline_model, split_slice_parts};
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
        self.compose_pending_with(kind, copy_spec, None, None)
    }

    /// 027 pending 入口：按显式 tick 步长/总组数组合。
    pub(super) fn compose_pending_copy_along(&mut self, spec: CopyAlongSpec) -> bool {
        self.compose_pending_with(ComposedKind::CopyAlong, spec, None, None)
    }

    /// 025 pending 入口：按一条显式切线切割唯一选中的形状（DRAW-025）
    pub(super) fn compose_pending_slice(&mut self, a: (f32, f32), b: (f32, f32)) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        self.compose_pending_with(ComposedKind::Slice, copy_spec, Some((a, b)), None)
    }

    /// 025 pending 入口：单形状 + 手画音符当刀（DRAW-025）
    pub(super) fn compose_pending_mould(
        &mut self,
        walls: Vec<lumino_editor_state::ShapeNote>,
    ) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        self.compose_pending_with(ComposedKind::Mould, copy_spec, None, Some(walls))
    }

    fn compose_pending_with(
        &mut self,
        kind: ComposedKind,
        copy_spec: CopyAlongSpec,
        slice_line: Option<((f32, f32), (f32, f32))>,
        mould_walls: Option<Vec<lumino_editor_state::ShapeNote>>,
    ) -> bool {
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
            // DEBT-14：抽屉插入只由 `PendingComposed::from_drawer` 产生，不走组合入口。
            ComposedKind::Insert => return false,
            ComposedKind::Live => {
                let Ok((model, _warnings)) = to_live(&models) else {
                    return false;
                };
                vec![ComposedPart::plain(
                    model,
                    notes.into_iter().flatten().collect(),
                )]
            }
            ComposedKind::Join => {
                let Some(joined) = join(&models, 1.0, 1.0) else {
                    return false;
                };
                vec![ComposedPart::plain(
                    joined.model,
                    notes.into_iter().flatten().collect(),
                )]
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
                vec![ComposedPart::plain(model, merged)]
            }
            ComposedKind::Slice => {
                if models.len() != 1 {
                    return false;
                }
                let model = &models[0];
                let Some((x0, y0, x1, y1)) = model.geometry.bounds() else {
                    return false;
                };
                // 未指定切线时退回 bbox 中心竖切（脚本/测试入口）
                let (raw_a, raw_b) = slice_line
                    .unwrap_or((((x0 + x1) / 2.0, y0 - 1.0), ((x0 + x1) / 2.0, y1 + 1.0)));
                let Some((a, b)) = extend_line(raw_a, raw_b, (x0, y0, x1, y1)) else {
                    return false;
                };
                let Some((first, second)) = split_slice_parts(model, a, b) else {
                    return false;
                };
                let (first_notes, second_notes) = split_notes_line(&notes[0], a, b);
                let closed = model.geometry.closed;
                let mut parts = vec![
                    ComposedPart::plain(polyline_model(first, track, model.k, closed), first_notes),
                    ComposedPart::plain(
                        polyline_model(second, track, model.k, closed),
                        second_notes,
                    ),
                ];
                attach_line_cut(&mut parts, model, a, b);
                parts
            }
            ComposedKind::Mould => {
                let (walls, dough_index) = match (mould_walls, models.len()) {
                    // 推荐入口：单形状 + 手画音符当刀
                    (Some(walls), 1) => (walls, 0),
                    // 兼容旧口径：两个待确认形状，靠前者当刀
                    (None, 2) => (notes[0].clone(), 1),
                    _ => return false,
                };
                let dough = &models[dough_index];
                let loops = vec![ring_of(dough)];
                let ppq = u32::from(self.editor_state.view.ppq);
                let pieces = mould_pieces(&walls, &notes[dough_index], &loops, ppq);
                if pieces.is_empty() {
                    return false;
                }
                pieces
                    .into_iter()
                    .enumerate()
                    .map(|(index, piece)| {
                        let model = polyline_model(vec![piece.ring], track, dough.k, true);
                        let mut part = ComposedPart::plain(model, piece.notes);
                        let mut cut = CutRef::for_piece(dough.clone(), &part.model, (0.0, None));
                        cut.knife = None;
                        cut.marks.push(CutMark {
                            id: index as u32,
                            at: index,
                            u: index as f32,
                            kind: CutKind::Split,
                            dir: (0.0, 1.0),
                        });
                        part.cut = Some(cut);
                        part
                    })
                    .collect()
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
                        ComposedPart::plain(model, part_notes)
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
                vec![ComposedPart::plain(model, composed_notes)]
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
                vec![ComposedPart::plain(model, composed_notes)]
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
            drawer: None,
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
}

/// 给沿直线切出的两片挂上片簿记（DRAW-025）
///
/// `part` 记「本片在原形状上的比例」：正侧（tick 较小）取 `0..0.5`，
/// 负侧取 `0.5..` 到末尾；`marks` 记录切线方向，供后续调试/展示。
fn attach_line_cut(
    parts: &mut [ComposedPart],
    whole: &lumino_core::shape::ShapeModel,
    a: (f32, f32),
    b: (f32, f32),
) {
    let dir = (b.0 - a.0, b.1 - a.1);
    let spans = [(0.0, Some(0.5)), (0.5, None)];
    for (index, part) in parts.iter_mut().enumerate() {
        let mut cut = CutRef::for_piece(whole.clone(), &part.model, spans[index]);
        cut.knife = Some(KnifeKind::Line);
        cut.marks.push(CutMark {
            id: index as u32,
            at: index,
            u: index as f32 * 0.5,
            kind: CutKind::Slice,
            dir,
        });
        part.cut = Some(cut);
    }
}
