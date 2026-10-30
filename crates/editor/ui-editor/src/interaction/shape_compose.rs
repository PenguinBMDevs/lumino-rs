//! 019–022 的 UI 组合入口（CP-4.5a）
//!
//! 在 `DrawnShape`（UI 选中对象）与 `ShapeModel`（019–022 算法输入）之间搭一条
//! 最小桥：组合时保留原音符，形状参数写入 `ShapeStore`，一次提交一个 group。

use lumino_core::shape::{
    BetweenParams, BetweenRole, BetweenSet, MergeParams, ShapeKind, ShapeModel, ShapeOp,
    StraightenParams, Sym, SymAxis, SymParams, VelocityEnv,
};
use lumino_core::tr;
use lumino_editor_state::shape_ops::{
    SymReject, between_blend, between_graph_at, copy_along_apply, cut_loops, gate_merge, join,
    symmetric_shape_with_reason, to_live,
};
use lumino_editor_state::{ComposedKind, DrawnShape, DrawnShapeSource, ShapeNote};

use crate::Editor;

use super::shape_bridge::{model_from_drawn, source_from_model};

impl Editor {
    /// 保证选中图形有对应的 `ShapeStore` 模型；返回模型 id
    pub(crate) fn ensure_model_for_drawn(&mut self, drawn_id: u64) -> Option<u64> {
        if let Some(model_id) = self.editor_state.shape_select.model_id_of(drawn_id)
            && self.editor_state.shape_store.model(model_id).is_some()
        {
            return Some(model_id);
        }
        let drawn = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|shape| shape.id == drawn_id)?
            .clone();
        if drawn.track == 0 || drawn.pending.is_some() {
            return None;
        }
        let model = model_from_drawn(&drawn, 1.0);
        let model_id = self.editor_state.shape_store.insert(model, 0);
        self.editor_state
            .shape_store
            .assign_notes(model_id, drawn.notes.clone());
        self.editor_state
            .shape_select
            .set_model_id(drawn_id, Some(model_id));
        Some(model_id)
    }

    /// 019：选中的图形折叠成一个 Live shape
    pub fn live_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Live);
        }
        let selected = self.selected_drawn_snapshot();
        if selected.is_empty() {
            return false;
        }
        let mut models = Vec::with_capacity(selected.len());
        for shape in &selected {
            let Some(model_id) = self.ensure_model_for_drawn(shape.id) else {
                return false;
            };
            let Some(model) = self.editor_state.shape_store.model(model_id).cloned() else {
                return false;
            };
            models.push(model);
        }
        let (model, _warnings) = match to_live(&models) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let notes = selected
            .iter()
            .flat_map(|shape| shape.notes.iter().copied())
            .collect::<Vec<_>>();
        self.replace_selected_with(model, notes, Some(selected))
    }

    /// 021：选中的图形连接成一条曲线
    pub fn join_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Join);
        }
        let selected = self.selected_drawn_snapshot();
        if selected.len() < 2 {
            return false;
        }
        let mut models = Vec::with_capacity(selected.len());
        for shape in &selected {
            let Some(model_id) = self.ensure_model_for_drawn(shape.id) else {
                return false;
            };
            let Some(model) = self.editor_state.shape_store.model(model_id).cloned() else {
                return false;
            };
            models.push(model);
        }
        let Some(joined) = join(&models, 1.0, 1.0) else {
            return false;
        };
        let notes = selected
            .iter()
            .flat_map(|shape| shape.notes.iter().copied())
            .collect::<Vec<_>>();
        self.replace_selected_with(joined.model, notes, Some(selected))
    }

    /// 022：两个图形按门控逐 key 行合并
    pub fn merge_selected_shapes(&mut self) -> bool {
        if self.pending_selection_only() {
            return self.compose_pending(ComposedKind::Merge);
        }
        let selected = self.selected_drawn_snapshot();
        if selected.len() != 2 {
            return false;
        }
        let Some(first_model_id) = self.ensure_model_for_drawn(selected[0].id) else {
            return false;
        };
        if self.ensure_model_for_drawn(selected[1].id).is_none() {
            return false;
        }
        let Some(mut first_model) = self.editor_state.shape_store.model(first_model_id).cloned()
        else {
            return false;
        };
        let params = MergeParams {
            stay: u32::try_from(first_model_id).unwrap_or(0),
            from_left: None,
            rest_slides: false,
        };
        let Some(outcome) = gate_merge(&selected[0].notes, &selected[1].notes, &params) else {
            return false;
        };
        let mut notes = selected[0].notes.clone();
        for note in &selected[1].notes {
            let shift = outcome.by_key.get(&note.key).copied().unwrap_or(0) + outcome.shift_all;
            let mut shifted = *note;
            shifted.tick = (shifted.tick + shift as f32).max(0.0);
            notes.push(shifted);
        }
        first_model.ops.push(ShapeOp::Merge(params));
        self.replace_selected_with(first_model, notes, None)
    }

    /// 023：切换对称（竖轴/横轴）。再次调用同一轴 = 关闭。
    ///
    /// 关闭不是失败，只有「拒绝」才返回 `Err`（附可展示原因，DRAW-043）。
    pub fn toggle_symmetry(&mut self, vertical: bool) -> Result<(), SymReject> {
        let result = self.toggle_symmetry_inner(vertical);
        if let Err(reason) = result {
            tracing::warn!(
                reason = sym_reject_message(reason),
                "半边对称被拒绝（DRAW-023）"
            );
        }
        result
    }

    fn toggle_symmetry_inner(&mut self, vertical: bool) -> Result<(), SymReject> {
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return Err(SymReject::NoSelection);
        };
        let params = SymParams {
            mode: Sym::Mirror,
            axis: if vertical {
                SymAxis::Vertical {
                    x: model
                        .geometry
                        .bounds()
                        .map(|b| (b.0 + b.2) / 2.0)
                        .unwrap_or(0.0),
                }
            } else {
                SymAxis::Horizontal {
                    y: model
                        .geometry
                        .bounds()
                        .map(|b| (b.1 + b.3) / 2.0)
                        .unwrap_or(0.0),
                }
            },
            source: 0,
        };
        let already = model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)));
        model.ops.retain(|op| !matches!(op, ShapeOp::Symmetry(_)));
        if !already {
            symmetric_shape_with_reason(&model, &params)?;
            model.ops.push(ShapeOp::Symmetry(params));
        }
        let updated = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        if updated {
            Ok(())
        } else {
            Err(SymReject::EditFailed)
        }
    }

    /// 024：显式开启/关闭扶正
    pub fn set_straighten(&mut self, on: bool) -> bool {
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Straighten(_)));
        if on {
            if !matches!(
                model.kind,
                ShapeKind::Freehand | ShapeKind::Polyline | ShapeKind::Line
            ) {
                return false;
            }
            model
                .ops
                .push(ShapeOp::Straighten(StraightenParams { level: 50 }));
        }
        let ok = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        ok
    }

    /// 025：选中一个图形，沿 bbox 中心竖切成两半
    pub fn slice_selected_vertical(&mut self) -> bool {
        let Some((_, _, model)) = self.selected_model_for_edit() else {
            return false;
        };
        let Some((x0, y0, x1, y1)) = model.geometry.bounds() else {
            return false;
        };
        let cut = (x0 + x1) / 2.0;
        let ring = geometry_ring(&model);
        let Some((left, right)) = cut_loops(&[ring], (cut, y0 - 1.0), (cut, y1 + 1.0)) else {
            return false;
        };
        let selected = self.selected_drawn_snapshot();
        let notes = selected[0].notes.clone();
        let (left_notes, right_notes) = split_notes_vertical(&notes, cut);
        let items = vec![
            ShapeItem {
                source: DrawnShapeSource::Polyline {
                    points: left.into_iter().flatten().collect(),
                },
                notes: left_notes,
                model: None,
            },
            ShapeItem {
                source: DrawnShapeSource::Polyline {
                    points: right.into_iter().flatten().collect(),
                },
                notes: right_notes,
                model: None,
            },
        ];
        self.replace_selected_with_many(items)
    }

    /// 026：两个选中图形生成 5 个 Between 步
    pub fn create_between_selected(&mut self) -> bool {
        let selected = self.selected_drawn_snapshot();
        if selected.len() != 2 {
            return false;
        }
        let Some(first_id) = self.ensure_model_for_drawn(selected[0].id) else {
            return false;
        };
        let Some(last_id) = self.ensure_model_for_drawn(selected[1].id) else {
            return false;
        };
        let Some(mut first) = self.editor_state.shape_store.model(first_id).cloned() else {
            return false;
        };
        let Some(mut last) = self.editor_state.shape_store.model(last_id).cloned() else {
            return false;
        };
        let set = BetweenSet {
            steps: 5,
            ..BetweenSet::default()
        };
        let group_id = 1;
        first.ops.push(ShapeOp::Between(BetweenParams {
            id: group_id,
            role: BetweenRole::First,
            at: Some(0.0),
            sig: None,
            push: None,
            vel: None,
            set: Some(set.clone()),
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
        let track = selected[0].track;
        let mut items = vec![
            ShapeItem {
                source: source_from_model(&first),
                notes: selected[0].notes.clone(),
                model: Some(first.clone()),
            },
            ShapeItem {
                source: source_from_model(&last),
                notes: selected[1].notes.clone(),
                model: Some(last.clone()),
            },
        ];
        for index in 1..=set.steps {
            let at = f32::from(index) / f32::from(set.steps + 1);
            let model = between_blend(&first, &last, between_graph_at(&set, at), set.rev);
            let notes = self.shape_notes_of(&model, track);
            items.insert(
                index as usize,
                ShapeItem {
                    source: source_from_model(&model),
                    notes,
                    model: Some(model),
                },
            );
        }
        self.replace_selected_with_many(items)
    }

    /// 027：选中一个图形，沿默认路径复制 3 份
    ///
    /// 副本是 `ShapeOp::CopyAlong` 这个 fx 页的产物：op 必须写回模型，
    /// 否则后续 `edit_shape` 按模型重生成时会把副本抹掉（DRAW-027 验收）。
    pub fn copy_along_selected(&mut self) -> bool {
        let Some((drawn_id, _, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        let Some(bounds) = model.geometry.bounds() else {
            return false;
        };
        let width = (bounds.2 - bounds.0).max(1.0);
        let params = lumino_core::shape::CopyPathParams {
            pts: vec![[width * 2.0, 0.0]],
            mode: lumino_core::shape::CopyMode::Count { n: 3 },
            grp: Some([bounds.0, bounds.1]),
            ..lumino_core::shape::CopyPathParams::default()
        };
        let Some(selected) = self
            .editor_state
            .shape_select
            .shapes()
            .iter()
            .find(|shape| shape.id == drawn_id)
            .cloned()
        else {
            return false;
        };
        let notes = copy_along_apply(&selected.notes, &params, 480.0);
        model.ops.push(ShapeOp::CopyAlong(params));
        // 替换旧模型：清 id 让 ShapeStore 分配新 id，避免与刚标记删除的旧条目同 id
        model.id = 0;
        self.replace_selected_with_many(vec![ShapeItem {
            source: selected.source,
            notes,
            model: Some(model),
        }])
    }

    /// 028：对选中图形应用渐强力度包络
    pub fn velocity_up_selected(&mut self) -> bool {
        let Some((drawn_id, model_id, mut model)) = self.selected_model_for_edit() else {
            return false;
        };
        model
            .ops
            .retain(|op| !matches!(op, ShapeOp::VelocityEnv(_)));
        model.ops.push(ShapeOp::VelocityEnv(VelocityEnv::new(vec![
            [0.0, 30.0],
            [1.0, 120.0],
        ])));
        let ok = self.edit_shape(model_id, model);
        self.refresh_drawn_source(drawn_id, model_id);
        ok
    }

    /// 019/021：还原最近一次组合前的图形
    pub fn split_composed_shape(&mut self) -> bool {
        let selected = self.selected_drawn_snapshot();
        if selected.len() != 1 {
            return false;
        }
        let id = selected[0].id;
        let Some(restored) = self.editor_state.shape_select.take_restore(id) else {
            return false;
        };
        self.push_history();
        let group = self.current_history_group();
        let notes = selected[0].notes.clone();
        self.remove_shape_notes(selected[0].track, &notes);
        self.editor_state.shape_select.mark_deleted(id);
        if let Some(model_id) = selected[0].model_id {
            self.editor_state.shape_store.mark_deleted(model_id, group);
            self.editor_state
                .shape_store
                .assign_notes(model_id, Vec::new());
        }
        for shape in restored {
            let mut shape = shape;
            shape.deleted = false;
            shape.ever_deleted = false;
            shape.hidden_by_creation = false;
            shape.model_id = None;
            let written = self.write_shape_notes(shape.track, &shape.notes);
            let new_id =
                self.editor_state
                    .shape_select
                    .add(shape.track, Some(group), shape.source, written);
            self.editor_state.shape_select.add_to_selection(new_id);
        }
        self.mark_notes_changed();
        true
    }

    fn replace_selected_with_many(&mut self, items: Vec<ShapeItem>) -> bool {
        let selected = self.selected_drawn_snapshot();
        if selected.is_empty() || items.is_empty() {
            return false;
        }
        let track = selected[0].track;
        self.push_history();
        let group = self.current_history_group();
        for shape in &selected {
            self.remove_shape_notes(shape.track, &shape.notes);
            self.editor_state.shape_select.mark_deleted(shape.id);
            if let Some(model_id) = shape.model_id {
                self.editor_state.shape_store.mark_deleted(model_id, group);
                self.editor_state
                    .shape_store
                    .assign_notes(model_id, Vec::new());
            }
        }
        for item in items {
            let written = self.write_shape_notes(track, &item.notes);
            let drawn_id = self.editor_state.shape_select.add(
                track,
                Some(group),
                item.source,
                written.clone(),
            );
            if let Some(model) = item.model {
                let model_id = self.editor_state.shape_store.insert(model, group);
                self.editor_state
                    .shape_store
                    .assign_notes(model_id, written);
                self.editor_state
                    .shape_select
                    .set_model_id(drawn_id, Some(model_id));
            }
            self.editor_state.shape_select.add_to_selection(drawn_id);
        }
        self.mark_notes_changed();
        true
    }

    fn selected_model_for_edit(&mut self) -> Option<(u64, u64, ShapeModel)> {
        let drawn = self.selected_drawn_snapshot();
        if drawn.len() != 1 {
            return None;
        }
        let drawn_id = drawn[0].id;
        let model_id = self.ensure_model_for_drawn(drawn_id)?;
        let model = self.editor_state.shape_store.model(model_id)?.clone();
        Some((drawn_id, model_id, model))
    }

    fn refresh_drawn_source(&mut self, drawn_id: u64, model_id: u64) {
        if let Some(model) = self.editor_state.shape_store.model(model_id).cloned() {
            let source = source_from_model(&model);
            self.editor_state.shape_select.set_source(drawn_id, source);
            self.grid_cache.clear();
        }
    }

    fn selected_drawn_snapshot(&self) -> Vec<DrawnShape> {
        self.editor_state
            .shape_select
            .selected_shapes()
            .cloned()
            .collect()
    }

    /// 用新模型替换当前选中集；`restore` 为可选还原快照
    fn replace_selected_with(
        &mut self,
        model: ShapeModel,
        notes: Vec<ShapeNote>,
        restore: Option<Vec<DrawnShape>>,
    ) -> bool {
        let selected = self.selected_drawn_snapshot();
        if selected.is_empty() {
            return false;
        }
        let track = selected[0].track;
        self.push_history();
        let group = self.current_history_group();
        for shape in &selected {
            self.remove_shape_notes(shape.track, &shape.notes);
            self.editor_state.shape_select.mark_deleted(shape.id);
            if let Some(model_id) = shape.model_id {
                self.editor_state.shape_store.mark_deleted(model_id, group);
                self.editor_state
                    .shape_store
                    .assign_notes(model_id, Vec::new());
            }
        }
        let source = source_from_model(&model);
        // 旧音符已删除；新形状的音符必须重新写入文档，不能只挂在 DrawnShape.notes 上。
        let written = self.write_shape_notes(track, &notes);
        let drawn_id =
            self.editor_state
                .shape_select
                .add(track, Some(group), source, written.clone());
        let model_id = self.editor_state.shape_store.insert(model, group);
        self.editor_state
            .shape_store
            .assign_notes(model_id, written);
        self.editor_state
            .shape_select
            .set_model_id(drawn_id, Some(model_id));
        self.editor_state.shape_select.select_only(drawn_id);
        if let Some(restore) = restore {
            self.editor_state
                .shape_select
                .set_restore(drawn_id, restore);
        }
        self.mark_notes_changed();
        true
    }
}

struct ShapeItem {
    source: DrawnShapeSource,
    notes: Vec<ShapeNote>,
    model: Option<ShapeModel>,
}

fn geometry_ring(model: &ShapeModel) -> Vec<(f32, f32)> {
    let mut ring = model
        .geometry
        .anchors
        .iter()
        .map(|anchor| anchor.pos)
        .collect::<Vec<_>>();
    if ring.is_empty()
        && let Some(rect) = model.geometry.rect
    {
        ring = vec![
            (rect.tick_lo, rect.key_lo),
            (rect.tick_hi, rect.key_lo),
            (rect.tick_hi, rect.key_hi),
            (rect.tick_lo, rect.key_hi),
        ];
    }
    if let Some(first) = ring.first().copied()
        && ring.last().copied() != Some(first)
    {
        ring.push(first);
    }
    ring
}

fn split_notes_vertical(notes: &[ShapeNote], cut: f32) -> (Vec<ShapeNote>, Vec<ShapeNote>) {
    let mut left = Vec::new();
    let mut right = Vec::new();
    for note in notes {
        let start = note.tick;
        let end = note.tick + note.length.max(0.0);
        if end <= cut {
            left.push(*note);
        } else if start >= cut {
            right.push(*note);
        } else {
            if cut > start {
                left.push(ShapeNote {
                    length: cut - start,
                    ..*note
                });
            }
            if end > cut {
                right.push(ShapeNote {
                    tick: cut,
                    length: end - cut,
                    ..*note
                });
            }
        }
    }
    (left, right)
}

/// 半边对称拒绝原因的用户可见文案（DRAW-043：拒绝必须说得出为什么）
pub(crate) fn sym_reject_message(reason: SymReject) -> &'static str {
    match reason {
        SymReject::NoSelection => tr!("请先选中一个形状"),
        SymReject::NotCurve => tr!("只有曲线形状支持半边对称"),
        SymReject::JoinedCurve => tr!("已连接的曲线不支持半边对称"),
        SymReject::TooFewAnchors => tr!("锚点太少，无法做半边对称"),
        SymReject::EvenAnchors => tr!("锚点数为偶数，中间锚点需落在对称轴上"),
        SymReject::InvalidParams => tr!("对称参数不合法"),
        SymReject::DegenerateAxis => tr!("对称轴退化，无法求解"),
        SymReject::NonFinite => tr!("对称结果包含非有限值"),
        SymReject::EditFailed => tr!("形状更新失败"),
    }
}

#[cfg(test)]
mod tests;
