//! DRAW-041C：Bake 前形状组合会话（pending 层）
//!
//! 组合只改 pending：来源从 owning tool 摘除，结果进入
//! `EditorState::pending_composed`，✓ 才写文档音符。

mod edit;
mod mould;
mod promote;
mod select;
mod slice;

use lumino_core::shape::{
    BetweenParams, BetweenRole, BetweenSet, MergeParams, ShapeModel, ShapeOp,
};
use lumino_editor_state::shape_ops::{
    extend_line, gate_merge, join, split_notes_line, sync_groups, to_live,
};

use self::mould::compose_mould;
use self::slice::{attach_line_cut, polyline_model, split_slice_parts};
use lumino_editor_state::{
    ComposedKind, ComposedPart, PendingComposed, PendingShapeRef, PendingSource,
};

use crate::Editor;
use crate::interaction::shape_compose::{CopyAlongSpec, MouldKnife};

impl Editor {
    /// 取一个可用于 pending 参数操作的单支组合；普通 pending 形状/曲线先提升。
    ///
    /// 返回 `(组合下标, 是否本次刚提升)`：
    /// - 刚提升：撤销走 `undo_pending_compose`（一步回到 pending 矢量）；
    /// - 已组合：调用方应压 `pending_edit_undo` 快照，撤销只回退本次参数变更。
    pub(super) fn pending_single_composed_for_ops(&mut self) -> Option<(usize, bool)> {
        let pending = {
            let mut selected = self.editor_state.shape_select.selected_shapes();
            let shape = selected.next()?;
            if selected.next().is_some() {
                return None;
            }
            shape.pending
        };
        match pending {
            Some(PendingShapeRef::Composed(index)) => {
                let composed = self.editor_state.pending_composed.get(index)?;
                (composed.results.len() == 1).then_some((index, false))
            }
            Some(PendingShapeRef::Shape(_) | PendingShapeRef::CurvePath(_)) => {
                if !self.promote_pending_for_edit() {
                    return None;
                }
                let pending = self
                    .editor_state
                    .shape_select
                    .selected_shapes()
                    .next()?
                    .pending;
                match pending {
                    Some(PendingShapeRef::Composed(index)) => Some((index, true)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// 用新模型替换 pending 组合的第一支并重算预览音符（不写文档历史）
    pub(super) fn update_pending_composed_model(&mut self, index: usize, model: ShapeModel) {
        let track = model.track as usize;
        let notes = self.part_notes_for_edit(&model, track);
        if let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        {
            part.model = model;
            part.notes = notes;
        }
        self.refresh_pending_mirror_geometry();
        self.mark_notes_changed();
        self.grid_cache.clear();
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

    /// 025 pending 入口：单形状 + 显式设置的刀（DRAW-025 / DEBT-17）
    pub(super) fn compose_pending_mould(&mut self, knife: MouldKnife) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        self.compose_pending_with(ComposedKind::Mould, copy_spec, None, Some(knife))
    }

    fn compose_pending_with(
        &mut self,
        kind: ComposedKind,
        copy_spec: CopyAlongSpec,
        slice_line: Option<((f32, f32), (f32, f32))>,
        mould_knife: Option<MouldKnife>,
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
            let model = self.model_from_source_at_view(&source, track, 1.0);
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

        // DEBT-17：Mould 分支产出的切口区间；其它组合保持为空。
        let mut mould_cuts: Vec<(u16, f32, f32)> = Vec::new();
        let mut results: Vec<ComposedPart> = match kind {
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
                let Some(knife) = mould_knife else {
                    return false;
                };
                if models.len() != 1 {
                    return false;
                }
                let ppq = u32::from(self.editor_state.view.ppq);
                let Some((parts, cuts)) = compose_mould(&knife, &models[0], &notes[0], track, ppq)
                else {
                    return false;
                };
                // 切口 = 空隙区间本身；刀在切完即清空，靠这份数据画到 ✓ / ×。
                mould_cuts = cuts;
                parts
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
        if refs
            .iter()
            .any(|reference| matches!(reference, PendingShapeRef::CurvePath(_)))
        {
            // 被消费的曲线已并入组合：历史快照不能再回放出旧路径。
            self.editor_state.line_tool.rebase_path_history();
        }

        let composed_index = self.editor_state.pending_composed.len();
        self.editor_state.pending_composed.push(PendingComposed {
            kind,
            results,
            sources,
            drawer: None,
            mould_cuts,
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
