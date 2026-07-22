use super::Editor;
use lumino_core::EditorTransform;

use std::collections::HashSet;

impl Editor {
    pub fn apply_speed_change(&mut self, speed_factor: f32) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self
            .editor_state
            .data
            .apply_speed_change(&selected, speed_factor);
        if result > 0 {
            self.mark_notes_changed();
        }
        result
    }

    pub fn apply_batch_edit(
        &mut self,
        velocity: &str,
        gate: &str,
        key: &str,
        tick: &str,
        max_key: u16,
    ) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self
            .editor_state
            .data
            .apply_batch_edit(&selected, velocity, gate, key, tick, max_key);
        if result > 0 {
            self.mark_notes_changed();
        }
        result
    }
}
