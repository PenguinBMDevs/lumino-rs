//! 音符分割与合并操作模块

use super::Editor;

impl Editor {
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        let result = self.editor_state.data.split_note(index, split_tick);
        if result {
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }

    pub fn glue_selected_notes(&mut self) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let result = self.editor_state.data.glue_selected_notes(&selected);
        if result > 0 {
            self.editor_state.interaction.selected_notes.clear();
            self.editor_state.interaction.hover_state = None;
            self.mark_notes_changed();
        }
        result
    }
}
