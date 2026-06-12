use super::Editor;

impl Editor {
    pub fn apply_speed_change(&mut self, speed_factor: f32) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let result = self.editor_state.data.apply_speed_change(&selected, speed_factor);
        if result > 0 {
            self.mark_notes_changed();
        }
        result
    }
}
