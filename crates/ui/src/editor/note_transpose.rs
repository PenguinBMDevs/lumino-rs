//! 音符移调操作模块
//!
//! 按半音阶对选中音符（或全部音符）进行上下移调。
//! 实现方式参考 `note_flip.rs` 的模式。

use super::Editor;

impl Editor {
    /// 按半音移调选中（或全部）音符
    ///
    /// # 参数
    /// * `semitones` - 半音数，正数升高，负数降低
    ///
    /// # 返回值
    /// 实际修改的音符数量
    pub fn transpose_selected(&mut self, semitones: i16) -> usize {
        let notes = &self.editor_state.data.notes;
        let selected: Vec<usize> = {
            let sel = &self.editor_state.interaction.selected_notes;
            if sel.is_empty() {
                // 无选中则移调所有音符
                (0..notes.len()).collect()
            } else {
                sel.iter().copied().collect()
            }
        };

        if selected.is_empty() {
            return 0;
        }

        // 推入历史记录
        self.push_history();

        let mut modified = 0usize;

        for &i in &selected {
            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                let new_key = (note.key as i16 + semitones).clamp(0, 255) as u16;
                if new_key != note.key {
                    note.key = new_key;
                    modified += 1;
                }
            }
        }

        if modified > 0 {
            self.mark_notes_changed();
        } else {
            // 没有实际修改，回退历史记录
            self.editor_state
                .data
                .history
                .undo(crate::editor::history::EditorSnapshot::new(
                    self.editor_state.data.notes.clone(),
                    self.editor_state.data.current_track,
                ));
        }

        modified
    }
}

#[cfg(test)]
mod tests {
    use super::Editor;
    use crate::editor::note::Note;

    fn create_test_editor_with_notes(notes: Vec<Note>) -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.data.notes = notes.into();
        editor
    }

    fn select_all_notes(editor: &mut Editor) {
        let count = editor.editor_state.data.notes.len();
        editor.editor_state.interaction.selected_notes = (0..count).collect();
    }

    // ========== 基本移调测试 ==========

    #[test]
    fn test_transpose_selected_up_one() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(100.0, 64, 100.0),
        ]);
        select_all_notes(&mut editor);

        let modified = editor.transpose_selected(1);

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].key, 61);
        assert_eq!(editor.editor_state.data.notes[1].key, 65);
    }

    #[test]
    fn test_transpose_selected_down_one() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(100.0, 64, 100.0),
        ]);
        select_all_notes(&mut editor);

        let modified = editor.transpose_selected(-1);

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].key, 59);
        assert_eq!(editor.editor_state.data.notes[1].key, 63);
    }

    #[test]
    fn test_transpose_octave_up() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 100.0)]);
        select_all_notes(&mut editor);

        editor.transpose_selected(12);

        assert_eq!(editor.editor_state.data.notes[0].key, 72);
    }

    #[test]
    fn test_transpose_octave_down() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 72, 100.0)]);
        select_all_notes(&mut editor);

        editor.transpose_selected(-12);

        assert_eq!(editor.editor_state.data.notes[0].key, 60);
    }

    // ========== 边界测试 ==========

    #[test]
    fn test_transpose_clamp_low() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 0, 100.0)]);
        select_all_notes(&mut editor);

        editor.transpose_selected(-1);

        // 0-1 = -1，应 clamp 到 0
        assert_eq!(editor.editor_state.data.notes[0].key, 0);
    }

    #[test]
    fn test_transpose_clamp_high() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 255, 100.0)]);
        select_all_notes(&mut editor);

        editor.transpose_selected(1);

        // 255+1=256，应 clamp 到 255
        assert_eq!(editor.editor_state.data.notes[0].key, 255);
    }

    // ========== 无选中测试 ==========

    #[test]
    fn test_transpose_no_selection_moves_all() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(100.0, 64, 100.0),
            Note::new(200.0, 67, 100.0),
        ]);
        // 不清除选中状态，确保没有选中时就是全部移调
        editor.editor_state.interaction.selected_notes.clear();

        let modified = editor.transpose_selected(1);

        assert_eq!(modified, 3);
        assert_eq!(editor.editor_state.data.notes[0].key, 61);
        assert_eq!(editor.editor_state.data.notes[1].key, 65);
        assert_eq!(editor.editor_state.data.notes[2].key, 68);
    }

    // ========== 撤销测试 ==========

    #[test]
    fn test_transpose_history() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(100.0, 64, 100.0),
        ]);
        select_all_notes(&mut editor);

        let initial_key_0 = editor.editor_state.data.notes[0].key;
        let initial_key_1 = editor.editor_state.data.notes[1].key;

        editor.transpose_selected(1);
        assert_eq!(editor.editor_state.data.notes[0].key, 61);
        assert_eq!(editor.editor_state.data.notes[1].key, 65);

        // Undo 恢复原始 key
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.editor_state.data.notes[0].key, initial_key_0);
        assert_eq!(editor.editor_state.data.notes[1].key, initial_key_1);
    }

    #[test]
    fn test_transpose_empty_notes() {
        let mut editor = Editor::new();
        let modified = editor.transpose_selected(1);
        assert_eq!(modified, 0);
    }

    #[test]
    fn test_transpose_zero_semitones() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 100.0)]);
        select_all_notes(&mut editor);

        let modified = editor.transpose_selected(0);
        assert_eq!(modified, 0);
        assert_eq!(editor.editor_state.data.notes[0].key, 60);
    }
}
