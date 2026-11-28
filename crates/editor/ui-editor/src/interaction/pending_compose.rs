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
    ComposedKind, ComposedPart, PendingComposed, PendingShapeRef, PendingSource, ShapeNote,
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
    }

    /// 把选中的 pending 形状/路径组合成一个 pending composed 结果。
    pub(super) fn compose_pending(&mut self, kind: ComposedKind) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        compose_pending_with(self, kind, copy_spec, None, None)
    }

    /// 027 pending 入口：按显式 tick 步长/总组数组合。
    pub(super) fn compose_pending_copy_along(&mut self, spec: CopyAlongSpec) -> bool {
        compose_pending_with(self, ComposedKind::CopyAlong, spec, None, None)
    }

    /// 025 pending 入口：按一条显式切线切割唯一选中的形状（DRAW-025）
    pub(super) fn compose_pending_slice(&mut self, a: (f32, f32), b: (f32, f32)) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        compose_pending_with(self, ComposedKind::Slice, copy_spec, Some((a, b)), None)
    }

    /// 025 pending 入口：单形状 + 显式设置的刀（DRAW-025 / DEBT-17）
    pub(super) fn compose_pending_mould(&mut self, knife: MouldKnife) -> bool {
        let copy_spec = CopyAlongSpec::half_beat(self.editor_state.view.ppq);
        compose_pending_with(self, ComposedKind::Mould, copy_spec, None, Some(knife))
    }

    /// 撤销上一次 Bake 前组合（回到 pending 矢量）
    pub(crate) fn undo_pending_compose(&mut self) -> bool {
        let Some(item) = self.editor_state.pending_composed.pop() else {
            return false;
        };
        for source in &item.sources {
            self.restore_pending_source(source.clone());
        }
        self.shape_tools.push_pending_composed_redo(item);
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        true
    }

    /// 重做上一次 Bake 前组合
    pub(crate) fn redo_pending_compose(&mut self) -> bool {
        let Some(item) = self.shape_tools.pending_composed_redo.pop() else {
            return false;
        };
        for source in &item.sources {
            self.remove_pending_source(source);
        }
        self.editor_state.pending_composed.push(item);
        self.shape_tools.clear_pending_edit_stacks();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        true
    }
}

/// 归一化后的组合输入：来源、模型、音符、引用与轨道。
struct ComposeInputs {
    refs: Vec<PendingShapeRef>,
    sources: Vec<PendingSource>,
    models: Vec<ShapeModel>,
    notes: Vec<Vec<ShapeNote>>,
    track: usize,
}

/// 合成分派参数：打包入口参数与归一化输入，避免长参数列表。
struct ComposeArgs<'a> {
    editor: &'a Editor,
    kind: ComposedKind,
    copy_spec: CopyAlongSpec,
    slice_line: Option<((f32, f32), (f32, f32))>,
    mould_knife: Option<MouldKnife>,
    inputs: ComposeInputs,
}

/// 合成产物：结果支、参与来源、引用与 Mould 切口区间。
struct ComposeOutcome {
    refs: Vec<PendingShapeRef>,
    sources: Vec<PendingSource>,
    results: Vec<ComposedPart>,
    cuts: Vec<(u16, f32, f32)>,
}

fn compose_pending_with(
    editor: &mut Editor,
    kind: ComposedKind,
    copy_spec: CopyAlongSpec,
    slice_line: Option<((f32, f32), (f32, f32))>,
    mould_knife: Option<MouldKnife>,
) -> bool {
    let Some(inputs) = collect_inputs(editor) else {
        return false;
    };
    let Some(outcome) = build_results(ComposeArgs {
        editor,
        kind,
        copy_spec,
        slice_line,
        mould_knife,
        inputs,
    }) else {
        return false;
    };
    commit_compose(
        editor,
        &outcome.refs,
        kind,
        outcome.results,
        outcome.sources,
        outcome.cuts,
    )
}

/// 输入归一化：把选中的 pending 引用转成来源、模型与预览音符。
///
/// 全程只读；任一引用无法解析即放弃整个组合，与逐项失败等价。
fn collect_inputs(editor: &Editor) -> Option<ComposeInputs> {
    let refs: Vec<PendingShapeRef> = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .filter_map(|shape| shape.pending)
        .collect();
    if refs.is_empty() {
        return None;
    }
    let track = editor.editor_state.data.current_track;
    let mut sources = Vec::with_capacity(refs.len());
    let mut models = Vec::with_capacity(refs.len());
    let mut notes = Vec::with_capacity(refs.len());
    for reference in &refs {
        let source = editor.pending_source(*reference)?;
        let model = editor.model_from_source_at_view(&source, track, 1.0);
        let model_notes = editor.shape_notes_of(&model, track);
        let pending_source = match reference {
            PendingShapeRef::Shape(i) => {
                PendingSource::Shape(editor.editor_state.shape_tool.shapes[*i].clone())
            }
            PendingShapeRef::CurvePath(i) => {
                PendingSource::CurvePath(editor.editor_state.line_tool.paths[*i].clone())
            }
            PendingShapeRef::BrushStroke(_) | PendingShapeRef::Composed(_) => return None,
        };
        sources.push(pending_source);
        models.push(model);
        notes.push(model_notes);
    }
    Some(ComposeInputs {
        refs,
        sources,
        models,
        notes,
        track,
    })
}

/// 按组合类型分派到各合成分支；切口区间只由 Mould 分支产出。
fn build_results(args: ComposeArgs<'_>) -> Option<ComposeOutcome> {
    let ComposeArgs {
        editor,
        kind,
        copy_spec,
        slice_line,
        mould_knife,
        inputs,
    } = args;
    let ComposeInputs {
        refs,
        sources,
        models,
        notes,
        track,
    } = inputs;
    // DEBT-17：Mould 分支产出的切口区间；其它组合保持为空。
    let mut cuts = Vec::new();
    let results = match kind {
        ComposedKind::Insert => return None,
        ComposedKind::Live => live_parts(&models, notes)?,
        ComposedKind::Join => join_parts(&models, notes)?,
        ComposedKind::Merge => merge_parts(&models, &notes)?,
        ComposedKind::Slice => slice_parts(&models, &notes, slice_line, track)?,
        ComposedKind::Mould => {
            let (parts, mould_cuts) = mould_parts(editor, &models, &notes, mould_knife, track)?;
            // 切口 = 空隙区间本身；刀在切完即清空，靠这份数据画到 ✓ / ×。
            cuts = mould_cuts;
            parts
        }
        ComposedKind::Between => between_parts(editor, &models, track)?,
        ComposedKind::CopyAlong => copy_along_parts(editor, models, copy_spec, track)?,
        ComposedKind::Velocity => velocity_parts(editor, models, track)?,
        // 只在「进入编辑图形」时直接构造，不走这里
        ComposedKind::Single => return None,
    };
    Some(ComposeOutcome {
        refs,
        sources,
        results,
        cuts,
    })
}

/// Live：多形状折叠，音符按输入顺序展平。
fn live_parts(models: &[ShapeModel], notes: Vec<Vec<ShapeNote>>) -> Option<Vec<ComposedPart>> {
    let Ok((model, _warnings)) = to_live(models) else {
        return None;
    };
    Some(vec![ComposedPart::plain(
        model,
        notes.into_iter().flatten().collect(),
    )])
}

/// Join：多形状连接为一条曲线，音符按输入顺序展平。
fn join_parts(models: &[ShapeModel], notes: Vec<Vec<ShapeNote>>) -> Option<Vec<ComposedPart>> {
    let joined = join(models, 1.0, 1.0)?;
    Some(vec![ComposedPart::plain(
        joined.model,
        notes.into_iter().flatten().collect(),
    )])
}

/// Merge：两形状门控合并，保留 stay 的整段再按 key 平移 slide。
fn merge_parts(models: &[ShapeModel], notes: &[Vec<ShapeNote>]) -> Option<Vec<ComposedPart>> {
    if models.len() != 2 {
        return None;
    }
    let params = MergeParams {
        stay: 0,
        from_left: None,
        rest_slides: false,
    };
    let outcome = gate_merge(&notes[0], &notes[1], &params)?;
    let mut merged = notes[0].clone();
    for note in &notes[1] {
        let shift = outcome.by_key.get(&note.key).copied().unwrap_or(0) + outcome.shift_all;
        let mut shifted = *note;
        shifted.tick = (shifted.tick + shift as f32).max(0.0);
        merged.push(shifted);
    }
    let mut model = models[0].clone();
    model.ops.push(ShapeOp::Merge(params));
    Some(vec![ComposedPart::plain(model, merged)])
}

/// Slice：单形状沿直线切成两片，几何与音符按同一条线拆分。
fn slice_parts(
    models: &[ShapeModel],
    notes: &[Vec<ShapeNote>],
    slice_line: Option<((f32, f32), (f32, f32))>,
    track: usize,
) -> Option<Vec<ComposedPart>> {
    if models.len() != 1 {
        return None;
    }
    let model = &models[0];
    let (x0, y0, x1, y1) = model.geometry.bounds()?;
    // 未指定切线时退回 bbox 中心竖切（脚本/测试入口）
    let (raw_a, raw_b) =
        slice_line.unwrap_or((((x0 + x1) / 2.0, y0 - 1.0), ((x0 + x1) / 2.0, y1 + 1.0)));
    let (a, b) = extend_line(raw_a, raw_b, (x0, y0, x1, y1))?;
    let (first, second) = split_slice_parts(model, a, b)?;
    let (first_notes, second_notes) = split_notes_line(&notes[0], a, b);
    let closed = model.geometry.closed;
    let mut parts = vec![
        ComposedPart::plain(polyline_model(first, track, model.k, closed), first_notes),
        ComposedPart::plain(polyline_model(second, track, model.k, closed), second_notes),
    ];
    attach_line_cut(&mut parts, model, a, b);
    Some(parts)
}

/// Mould 分支产物：结果支与切口区间。
type MouldOutput = (Vec<ComposedPart>, Vec<(u16, f32, f32)>);

/// Mould：单面团加刀，返回两片与切口区间。
fn mould_parts(
    editor: &Editor,
    models: &[ShapeModel],
    notes: &[Vec<ShapeNote>],
    mould_knife: Option<MouldKnife>,
    track: usize,
) -> Option<MouldOutput> {
    let knife = mould_knife?;
    if models.len() != 1 {
        return None;
    }
    let ppq = u32::from(editor.editor_state.view.ppq);
    compose_mould(&knife, &models[0], &notes[0], track, ppq)
}

/// Between：两形状生成首/步/末组，步由纯函数同步生成。
fn between_parts(
    editor: &Editor,
    models: &[ShapeModel],
    track: usize,
) -> Option<Vec<ComposedPart>> {
    if models.len() != 2 {
        return None;
    }
    let set = BetweenSet {
        steps: 5,
        ..BetweenSet::default()
    };
    let group_id = editor.next_between_group_id();
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
    Some(
        shapes
            .into_iter()
            .map(|model| {
                let part_notes = editor.shape_notes_of(&model, track);
                ComposedPart::plain(model, part_notes)
            })
            .collect(),
    )
}

/// CopyAlong：单形状沿默认路径复制，音符由新模型重新生成。
fn copy_along_parts(
    editor: &Editor,
    mut models: Vec<ShapeModel>,
    copy_spec: CopyAlongSpec,
    track: usize,
) -> Option<Vec<ComposedPart>> {
    if models.len() != 1 {
        return None;
    }
    let mut model = models.pop()?;
    let params = Editor::default_copy_along_params(&model, copy_spec)?;
    model.ops.push(ShapeOp::CopyAlong(params));
    let composed_notes = editor.shape_notes_of(&model, track);
    Some(vec![ComposedPart::plain(model, composed_notes)])
}

/// Velocity：单形状套默认渐强包络，音符由新模型重新生成。
fn velocity_parts(
    editor: &Editor,
    mut models: Vec<ShapeModel>,
    track: usize,
) -> Option<Vec<ComposedPart>> {
    if models.len() != 1 {
        return None;
    }
    let mut model = models.pop()?;
    model
        .ops
        .retain(|op| !matches!(op, ShapeOp::VelocityEnv(_)));
    model
        .ops
        .push(ShapeOp::VelocityEnv(Editor::rising_velocity_env()));
    let composed_notes = editor.shape_notes_of(&model, track);
    Some(vec![ComposedPart::plain(model, composed_notes)])
}

/// 提交组合：清 id、按倒序摘除来源、压入 pending 并重建镜像。
///
/// 顺序固定：先清结果 id，再摘除来源，最后才登记组合与选中状态。
fn commit_compose(
    editor: &mut Editor,
    refs: &[PendingShapeRef],
    kind: ComposedKind,
    mut results: Vec<ComposedPart>,
    sources: Vec<PendingSource>,
    cuts: Vec<(u16, f32, f32)>,
) -> bool {
    for part in &mut results {
        part.model.id = 0;
    }

    let mut shape_indices = Vec::new();
    let mut curve_indices = Vec::new();
    for reference in refs {
        match reference {
            PendingShapeRef::Shape(i) => shape_indices.push(*i),
            PendingShapeRef::CurvePath(i) => curve_indices.push(*i),
            _ => return false,
        }
    }
    shape_indices.sort_unstable_by(|a, b| b.cmp(a));
    curve_indices.sort_unstable_by(|a, b| b.cmp(a));
    for index in shape_indices {
        editor.editor_state.shape_tool.shapes.remove(index);
    }
    for index in curve_indices {
        editor.editor_state.line_tool.paths.remove(index);
    }
    if refs
        .iter()
        .any(|reference| matches!(reference, PendingShapeRef::CurvePath(_)))
    {
        // 被消费的曲线已并入组合：历史快照不能再回放出旧路径。
        editor.editor_state.line_tool.rebase_path_history();
    }

    let composed_index = editor.editor_state.pending_composed.len();
    editor.editor_state.pending_composed.push(PendingComposed {
        kind,
        results,
        sources,
        drawer: None,
        mould_cuts: cuts,
    });
    editor.shape_tools.clear_pending_stacks();
    editor.editor_state.shape_select.remove_pending_all();
    editor.rebuild_pending_mirrors();
    if let Some(id) = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Composed(composed_index))
    {
        editor.editor_state.shape_select.select_only(id);
    }
    true
}
