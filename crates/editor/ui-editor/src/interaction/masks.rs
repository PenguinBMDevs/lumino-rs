//! Mask 的编辑器接线：收集 group、预览层数据、Apply 实体化
//!
//! 遮罩只影响显示：`mask_previews` 返回 ghost/visible 供渲染层叠加；
//! `apply_mask` 才把可见片段写成新形状（原形状与遮罩不变）。

use std::collections::HashMap;

use lumino_core::shape::ShapeOp;
use lumino_editor_state::PendingShapeRef;
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{
    MaskMember, MaskPreview, UnitArea, masks as mask_ops, member_of, unit_area,
};

use crate::Editor;

mod apply;
mod params;

pub use params::MaskEditTarget;

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
            let mut model = self.model_from_source_at_view(&source, track, 1.0);
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
                let model = self.model_from_source_at_view(&source, track, 1.0);
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
                let model = self.model_from_source_at_view(&source, track, 1.0);
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

#[cfg(test)]
mod tests;
