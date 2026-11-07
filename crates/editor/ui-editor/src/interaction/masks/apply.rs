//! Mask Apply 实体化（DRAW-010）
//!
//! 从 `masks.rs` 拆出（文件长度纪律）。

use lumino_core::shape::{ShapeKind, ShapeModel, ShapeStyle};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::masks as mask_ops;

use crate::Editor;

impl Editor {
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
