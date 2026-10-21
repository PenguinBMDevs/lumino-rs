//! Mask 的编辑器接线：收集 group、预览层数据、Apply 实体化
//!
//! 遮罩只影响显示：`mask_previews` 返回 ghost/visible 供渲染层叠加；
//! `apply_mask` 才把可见片段写成新形状（原形状与遮罩不变）。

use std::collections::HashMap;

use lumino_core::shape::{MaskParams, ShapeKind, ShapeModel, ShapeOp, ShapeStyle};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{
    MaskPreview, UnitArea, masks as mask_ops, member_of, unit_area,
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
        out
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
