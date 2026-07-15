//! 音符移调操作模块

use super::Editor;
use lumino_core::EditorTransform;

impl Editor {
    pub fn transpose_selected(&mut self, semitones: i16) -> usize {
        let selected = self.editor_state.interaction.selected_notes.clone();
        let result = self.editor_state.data.transpose(&selected, semitones);
        if result > 0 {
            self.mark_notes_changed();
        }
        result
    }
}
