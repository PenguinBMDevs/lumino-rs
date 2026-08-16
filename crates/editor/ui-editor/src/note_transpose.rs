//! 音符移调操作模块

use super::Editor;
use lumino_editor_state::EditorTransform;

use std::collections::HashSet;

impl Editor {
    pub fn transpose_selected(&mut self, semitones: i16) -> usize {
        let selected: HashSet<usize> = self.get_selected_indices().into_iter().collect();
        let result = self.editor_state.data.transpose(&selected, semitones);
        if result > 0 {
            self.mark_notes_changed();
        }
        result
    }
}
