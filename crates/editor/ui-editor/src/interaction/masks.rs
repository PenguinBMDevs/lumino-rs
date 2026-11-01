//! Mask 的编辑器接线：收集 group、预览层数据、Apply 实体化
//!
//! 遮罩只影响显示：`mask_previews` 返回 ghost/visible 供渲染层叠加；
//! `apply_mask` 才把可见片段写成新形状（原形状与遮罩不变）。

use std::collections::HashMap;

use lumino_core::shape::{MaskParams, ShapeKind, ShapeModel, ShapeOp, ShapeStyle};
use lumino_editor_state::PendingShapeRef;
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{
    MaskMember, MaskPreview, UnitArea, masks as mask_ops, member_of, unit_area,
};

use crate::Editor;

impl Editor {
    /// 按 Shapes 列表顺序收集 mask group（`group=None` 各自独立）
    fn mask_units(&self) -> Vec<(Vec<u64>, UnitArea)> {
        let ppq = self.editor_state.view.ppq as f64;
        let nkeys = self.editor_state.view.key_count;
        let mut units: Vec<(Vec<u64>, Vec<_>)> = Vec::new();
        let mut groups: HashMap<u32, usize> = HashMap::new();
        for entry in self
            .editor_state
            .shape_store
            .iter()
            .filter(|entry| entry.is_visible())
        {
            let Some(params) = mask_ops::params_of(&entry.model) else {
                continue;
            };
            let source = if entry.source_notes.is_empty() {
                &entry.notes
            } else {
                &entry.source_notes
            };
            let Some(member) = member_of(&entry.model, source, ppq, nkeys) else {
                continue;
            };
            match params.group {
                Some(group) => match groups.get(&group) {
                    Some(&index) => {
                        units[index].0.push(entry.model.id);
                        units[index].1.push(member);
                    }
                    None => {
                        groups.insert(group, units.len());
                        units.push((vec![entry.model.id], vec![member]));
                    }
                },
                None => units.push((vec![entry.model.id], vec![member])),
            }
        }
        units
            .into_iter()
            .map(|(ids, members)| (ids, unit_area(&members)))
            .collect()
    }

    /// 渲染层预览数据：每个被遮罩触及的非 mask 形状一份 ghost + visible
    pub fn mask_previews(&self) -> Vec<MaskPreview> {
        let mut out = self.baked_mask_previews();
        out.extend(self.pending_mask_previews());
        out
    }

    fn baked_mask_previews(&self) -> Vec<MaskPreview> {
        let units = self.mask_units();
        if units.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (_, unit) in units {
            for entry in self
                .editor_state
                .shape_store
                .iter()
                .filter(|entry| entry.is_visible())
            {
                if mask_ops::params_of(&entry.model).is_some() {
                    continue; // 遮罩不能遮罩其它遮罩
                }
                if let Some(preview) = mask_ops::preview(&entry.notes, &unit) {
                    out.push(preview);
                }
            }
        }
        out
    }

    /// pending 形状上的 Mask 标记 → 区域；`group` 语义与已 Bake 一致。
    fn pending_mask_units(&self) -> Vec<UnitArea> {
        if !self.pending_preview_visible(lumino_message::Tool::Shape) {
            return Vec::new();
        }
        let ppq = f64::from(self.editor_state.view.ppq);
        let nkeys = self.editor_state.view.key_count;
        let track = self.editor_state.data.current_track;
        let mut units: Vec<(Vec<MaskMember>, Option<u32>)> = Vec::new();
        for (index, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
            let Some(params) = shape.mask.clone() else {
                continue;
            };
            let Some(source) = self.pending_source(PendingShapeRef::Shape(index)) else {
                continue;
            };
            let mut model =
                crate::interaction::shape_bridge::model_from_source(&source, track, 1.0);
            model.ops.push(ShapeOp::Mask(params.clone()));
            let notes = self.shape_notes_of(&model, track);
            let Some(member) = member_of(&model, &notes, ppq, nkeys) else {
                continue;
            };
            match params.group {
                Some(group) => {
                    if let Some((members, _)) = units
                        .iter_mut()
                        .find(|(_, existing)| *existing == Some(group))
                    {
                        members.push(member);
                    } else {
                        units.push((vec![member], Some(group)));
                    }
                }
                None => units.push((vec![member], None)),
            }
        }
        // 020 提升出来的单支组合同样能带 Mask
        for composed in &self.editor_state.pending_composed {
            for part in &composed.results {
                let Some(params) = mask_ops::params_of(&part.model) else {
                    continue;
                };
                let Some(member) = member_of(&part.model, &part.notes, ppq, nkeys) else {
                    continue;
                };
                match params.group {
                    Some(group) => {
                        if let Some((members, _)) = units
                            .iter_mut()
                            .find(|(_, existing)| *existing == Some(group))
                        {
                            members.push(member);
                        } else {
                            units.push((vec![member], Some(group)));
                        }
                    }
                    None => units.push((vec![member], None)),
                }
            }
        }
        units
            .into_iter()
            .map(|(members, _)| unit_area(&members))
            .collect()
    }

    fn pending_mask_previews(&self) -> Vec<MaskPreview> {
        let units = self.pending_mask_units();
        if units.is_empty() {
            return Vec::new();
        }
        let track = self.editor_state.data.current_track;
        let mut out = Vec::new();
        for unit in units {
            for (index, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
                if shape.mask.is_some() {
                    continue;
                }
                let Some(source) = self.pending_source(PendingShapeRef::Shape(index)) else {
                    continue;
                };
                let model =
                    crate::interaction::shape_bridge::model_from_source(&source, track, 1.0);
                let notes = self.shape_notes_of(&model, track);
                if let Some(preview) = mask_ops::preview(&notes, &unit) {
                    out.push(preview);
                }
            }
            for composed in &self.editor_state.pending_composed {
                for part in &composed.results {
                    if let Some(preview) = mask_ops::preview(&part.notes, &unit) {
                        out.push(preview);
                    }
                }
            }
        }
        out
    }

    /// silent mask（own=false）的边框音符：渲染层用灰色叠加，代替虚线示意
    pub fn mask_outline_notes(&self) -> Vec<ShapeNote> {
        let mut out = Vec::new();
        for entry in self
            .editor_state
            .shape_store
            .iter()
            .filter(|entry| entry.is_visible())
        {
            let Some(params) = mask_ops::params_of(&entry.model) else {
                continue;
            };
            if params.own {
                continue;
            }
            let source = if entry.source_notes.is_empty() {
                &entry.notes
            } else {
                &entry.source_notes
            };
            out.extend_from_slice(source);
        }
        // DRAW-010 pending：silent mask（own=false）用虚线段预览，不写文档
        if self.pending_preview_visible(lumino_message::Tool::Shape) {
            let track = self.editor_state.data.current_track;
            for (index, shape) in self.editor_state.shape_tool.shapes.iter().enumerate() {
                let Some(params) = &shape.mask else {
                    continue;
                };
                if params.own {
                    continue;
                }
                let Some(source) = self.pending_source(PendingShapeRef::Shape(index)) else {
                    continue;
                };
                let model =
                    crate::interaction::shape_bridge::model_from_source(&source, track, 1.0);
                out.extend(self.shape_notes_of(&model, track));
            }
            // 020 单支组合的 own=false 同样用虚线段示意
            for composed in &self.editor_state.pending_composed {
                for part in &composed.results {
                    if mask_ops::params_of(&part.model).is_some_and(|params| params.own) {
                        continue;
                    }
                    if mask_ops::params_of(&part.model).is_some() {
                        out.extend_from_slice(&part.notes);
                    }
                }
            }
        }
        out
    }
}

/// 遮罩参数面板的编辑目标
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskEditTarget {
    /// 待确认形状（`ShapeInstance.mask`）：预览即时生效，不写文档历史
    Pending {
        /// `shape_tool.shapes` 下标
        index: usize,
    },
    /// 单支待确认组合（020 提升出来的 `Single`）：同样即时预览
    PendingComposed {
        /// `EditorState::pending_composed` 下标
        index: usize,
    },
    /// 已 Bake 形状（`ShapeOp::Mask`）：面板改草稿，Apply 时一次 `edit_shape`
    Baked {
        /// ShapeStore 模型 id
        model_id: u64,
    },
}

impl Editor {
    /// 当前选中对象的遮罩编辑目标（须单选中；多支组合/曲线路径不支持）
    pub fn mask_edit_target(&mut self) -> Option<MaskEditTarget> {
        if let Some(shape) = self.editor_state.shape_select.selected_shapes().next() {
            if self.editor_state.shape_select.selection_len() != 1 {
                return None;
            }
            match shape.pending {
                Some(PendingShapeRef::Shape(index)) => {
                    return Some(MaskEditTarget::Pending { index });
                }
                Some(PendingShapeRef::Composed(index)) => {
                    let composed = self.editor_state.pending_composed.get(index)?;
                    if composed.kind != lumino_editor_state::ComposedKind::Single {
                        return None;
                    }
                    return Some(MaskEditTarget::PendingComposed { index });
                }
                Some(_) => return None,
                None => {}
            }
        }
        let (_, model_id, _) = self.selected_model_for_edit()?;
        Some(MaskEditTarget::Baked { model_id })
    }

    /// 目标当前的遮罩参数（无遮罩 → `None`）
    pub fn mask_params_of(&self, target: MaskEditTarget) -> Option<MaskParams> {
        match target {
            MaskEditTarget::Pending { index } => self
                .editor_state
                .shape_tool
                .shapes
                .get(index)
                .and_then(|shape| shape.mask.clone()),
            MaskEditTarget::PendingComposed { index } => self
                .editor_state
                .pending_composed
                .get(index)
                .and_then(|composed| composed.results.first())
                .and_then(|part| mask_ops::params_of(&part.model).cloned()),
            MaskEditTarget::Baked { model_id } => self
                .editor_state
                .shape_store
                .model(model_id)
                .and_then(|model| mask_ops::params_of(model).cloned()),
        }
    }

    /// 待确认形状即时设置遮罩（`None` = 清除）；已 Bake 请走 [`Self::set_mask`]
    pub fn set_pending_mask(&mut self, index: usize, params: Option<MaskParams>) -> bool {
        let Some(shape) = self.editor_state.shape_tool.shapes.get_mut(index) else {
            return false;
        };
        if shape.mask == params {
            return false;
        }
        shape.mask = params;
        self.rebuild_pending_mirrors();
        self.mark_notes_changed();
        self.grid_cache.clear();
        true
    }

    /// 单支待确认组合的遮罩：改写模型上的 `Mask` op 并重算预览音符
    pub fn set_pending_composed_mask(&mut self, index: usize, params: Option<MaskParams>) -> bool {
        let Some(composed) = self.editor_state.pending_composed.get(index) else {
            return false;
        };
        let Some(mut model) = composed.results.first().map(|part| part.model.clone()) else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Mask(_)));
        if let Some(params) = params {
            model.ops.push(ShapeOp::Mask(params));
        }
        let track = model.track as usize;
        let notes = self.shape_notes_of(&model, track);
        let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        else {
            return false;
        };
        part.model = model;
        part.notes = notes;
        self.refresh_pending_mirror_geometry();
        self.mark_notes_changed();
        self.grid_cache.clear();
        true
    }

    /// 给形状设置/移除 Mask 标记（供工具栏/面板接线；一次 edit_shape 撤销单元）
    pub fn set_mask(&mut self, id: u64, params: Option<MaskParams>) -> bool {
        let Some(mut model) = self.editor_state.shape_store.model(id).cloned() else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Mask(_)));
        if let Some(params) = params {
            model.ops.push(ShapeOp::Mask(params));
        }
        self.edit_shape(id, model)
    }

    /// 切换当前选中形状的 Mask 标记：pending 形状改 `ShapeInstance.mask`；
    /// 已 Bake 形状走 `set_mask`（一次 edit_shape 撤销单元）。
    pub fn toggle_mask_selected(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.toggle_pending_mask();
        }
        let Some((_, model_id, model)) = self.selected_model_for_edit() else {
            return false;
        };
        let params = match mask_ops::params_of(&model) {
            Some(_) => None,
            None => Some(MaskParams::default()),
        };
        self.set_mask(model_id, params)
    }

    fn toggle_pending_mask(&mut self) -> bool {
        let index = {
            let mut selected = self.editor_state.shape_select.selected_shapes();
            let Some(shape) = selected.next() else {
                return false;
            };
            if selected.next().is_some() {
                return false;
            }
            let Some(PendingShapeRef::Shape(index)) = shape.pending else {
                return false;
            };
            index
        };
        {
            let Some(shape) = self.editor_state.shape_tool.shapes.get_mut(index) else {
                return false;
            };
            shape.mask = if shape.mask.is_some() {
                None
            } else {
                Some(MaskParams::default())
            };
        }
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        self.mark_notes_changed();
        true
    }

    /// Apply mask：把该 group（或独立遮罩）透出来的音符实体化为新形状
    ///
    /// 原遮罩与被遮形状都不变（像盖章）；返回新形状 id。
    pub fn apply_mask(&mut self, mask_id: u64) -> Option<u64> {
        let (_, unit) = self
            .mask_units()
            .into_iter()
            .find(|(ids, _)| ids.contains(&mask_id))?;
        let entries: Vec<(u64, ShapeModel, Vec<ShapeNote>)> = self
            .editor_state
            .shape_store
            .iter()
            .filter(|entry| entry.is_visible())
            .map(|entry| (entry.model.id, entry.model.clone(), entry.notes.clone()))
            .collect();
        let mut visible = Vec::new();
        for (_, model, notes) in &entries {
            if mask_ops::params_of(model).is_some() {
                continue;
            }
            if let Some(preview) = mask_ops::preview(notes, &unit) {
                visible.extend(preview.visible);
            }
        }
        if visible.is_empty() {
            return None;
        }
        let mask_model = entries
            .iter()
            .find(|(id, _, _)| *id == mask_id)
            .map(|(_, model, _)| model.clone())?;
        let mut model = ShapeModel::new(0, mask_model.track, ShapeKind::Custom, mask_model.k);
        model.name = "Mask apply".to_string();
        model.style = ShapeStyle::default();

        self.push_history();
        let group = self.current_history_group();
        let written = self.write_shape_notes(0, &visible);
        let id = self.editor_state.shape_store.insert(model, group);
        self.editor_state.shape_store.assign_notes(id, written);
        self.editor_state.data.mark_current_track_changed();
        self.mark_notes_changed();
        Some(id)
    }
}

#[cfg(test)]
mod tests;
