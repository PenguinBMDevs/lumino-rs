//! 019–022 的 UI 组合入口（CP-4.5a）
//!
//! 在 `DrawnShape`（UI 选中对象）与 `ShapeModel`（019–022 算法输入）之间搭一条
//! 最小桥：组合时保留原音符，形状参数写入 `ShapeStore`，一次提交一个 group。

use lumino_core::shape::{MergeParams, ShapeModel, ShapeOp};
use lumino_core::tr;
use lumino_editor_state::shape_ops::{SymReject, gate_merge, join, to_live};
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

mod ops;
