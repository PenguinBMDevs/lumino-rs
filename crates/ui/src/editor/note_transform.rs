use super::Editor;

impl Editor {
    /// 对选中的音符应用变速操作
    ///
    /// # 行为
    /// - 有选中音符：仅修改选中音符
    /// - 无选中音符：修改全部音符
    /// - 以最早音符的 tick 为锚点，所有音符的 tick 和 length 同时按比例缩放
    /// - 保持音符之间的相对间距比例，尾部贴合的音符变速后仍然贴合
    /// - 音符长度最小值 clamp 到 1 tick
    ///
    /// # 返回值
    /// 实际修改的音符数量
    pub fn apply_speed_change(&mut self, speed_factor: f32) -> usize {
        let notes = &self.editor_state.data.notes;
        if notes.is_empty() {
            return 0;
        }

        // 获取目标音符索引
        let target_indices: Vec<usize> = {
            let selected = &self.editor_state.interaction.selected_notes;
            if selected.is_empty() {
                (0..notes.len()).collect()
            } else {
                let mut v: Vec<usize> = selected.iter().copied().collect();
                v.sort();
                v
            }
        };

        if target_indices.is_empty() {
            return 0;
        }

        // 找出选中音符中最小的 tick（锚点）
        let min_tick = target_indices
            .iter()
            .filter_map(|i| notes.get(*i).map(|n| n.tick))
            .fold(f32::INFINITY, f32::min);

        if min_tick.is_infinite() {
            return 0;
        }

        // 推入历史记录
        self.push_history();

        // MIDI 音符最小长度为 1 tick，避免长度为 0 的音符
        const MIN_NOTE_LENGTH: f32 = 1.0;
        let mut modified = 0usize;

        for &i in &target_indices {
            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                let new_tick = min_tick + (note.tick - min_tick) * speed_factor;
                let new_length = (note.length * speed_factor).max(MIN_NOTE_LENGTH);

                if (new_tick - note.tick).abs() > f32::EPSILON
                    || (new_length - note.length).abs() > f32::EPSILON
                {
                    note.tick = new_tick;
                    note.length = new_length;
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

    /// 垂直翻转选中音符
    ///
    /// 根据选中音符的key范围计算水平中轴，将每个音符沿中轴镜像翻转。
    /// 例如：key范围为 [60, 64]，中轴为 62，则 60→64, 61→63, 62→62, 63→61, 64→60。
    ///
    /// # 返回值
    /// 实际修改的音符数量
    pub fn flip_selected_notes_vertical(&mut self) -> usize {
        let selected: Vec<usize> = self
            .editor_state
            .interaction
            .selected_notes
            .iter()
            .copied()
            .collect();
        if selected.is_empty() {
            return 0;
        }

        let notes = &self.editor_state.data.notes;

        // 计算选中音符的key范围
        let mut min_key = u16::MAX;
        let mut max_key = u16::MIN;
        let mut has_valid = false;

        for &i in &selected {
            if let Some(note) = notes.get(i) {
                min_key = min_key.min(note.key);
                max_key = max_key.max(note.key);
                has_valid = true;
            }
        }

        if !has_valid {
            return 0;
        }

        // 计算中轴（水平中轴 = 垂直方向的中心）
        let center_key = (min_key as f32 + max_key as f32) / 2.0;

        // 推入历史记录
        self.push_history();

        let mut modified = 0usize;
        let max_key_index = (self.editor_state.view.visible_key_count - 1) as f32;

        for &i in &selected {
            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                let new_key_f = 2.0 * center_key - note.key as f32;
                let new_key = new_key_f.round().clamp(0.0, max_key_index) as u16;

                if new_key != note.key {
                    note.key = new_key;
                    modified += 1;
                }
            }
        }

        if modified > 0 {
            self.mark_notes_changed();
        } else {
            // 没有实际修改（例如所有音符都在中轴上），回退历史记录
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

    #[test]
    fn test_flip_vertical_basic() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(0.0, 64, 100.0),
        ]);
        editor.select_all_notes();

        let modified = editor.flip_selected_notes_vertical();

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].key, 64);
        assert_eq!(editor.editor_state.data.notes[1].key, 60);
    }

    #[test]
    fn test_flip_vertical_with_odd_count() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 59, 100.0),
            Note::new(0.0, 60, 100.0),
            Note::new(0.0, 61, 100.0),
        ]);
        editor.select_all_notes();

        let modified = editor.flip_selected_notes_vertical();

        assert_eq!(modified, 2); // 60 stays at center
        assert_eq!(editor.editor_state.data.notes[0].key, 61);
        assert_eq!(editor.editor_state.data.notes[1].key, 60);
        assert_eq!(editor.editor_state.data.notes[2].key, 59);
    }

    #[test]
    fn test_flip_vertical_no_selection() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 100.0)]);

        let modified = editor.flip_selected_notes_vertical();

        assert_eq!(modified, 0);
        assert_eq!(editor.editor_state.data.notes[0].key, 60);
    }

    #[test]
    fn test_flip_vertical_clamps_to_range() {
        let mut editor =
            create_test_editor_with_notes(vec![Note::new(0.0, 0, 100.0), Note::new(0.0, 2, 100.0)]);
        // visible_key_count default is 128, so max_key_index = 127
        editor.select_all_notes();

        let modified = editor.flip_selected_notes_vertical();

        assert_eq!(modified, 2);
        // 0 and 2 around center 1, flipped stays within range
        assert_eq!(editor.editor_state.data.notes[0].key, 2);
        assert_eq!(editor.editor_state.data.notes[1].key, 0);
    }

    #[test]
    fn test_flip_vertical_preserves_tick_and_length() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(100.0, 60, 50.0),
            Note::new(200.0, 64, 75.0),
        ]);
        editor.select_all_notes();

        editor.flip_selected_notes_vertical();

        assert_eq!(editor.editor_state.data.notes[0].tick, 100.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 50.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 200.0);
        assert_eq!(editor.editor_state.data.notes[1].length, 75.0);
    }

    #[test]
    fn test_flip_vertical_history() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(0.0, 64, 100.0),
        ]);
        editor.select_all_notes();

        let initial_key_0 = editor.editor_state.data.notes[0].key;
        let initial_key_1 = editor.editor_state.data.notes[1].key;

        editor.flip_selected_notes_vertical();
        assert_eq!(editor.editor_state.data.notes[0].key, 64);
        assert_eq!(editor.editor_state.data.notes[1].key, 60);

        // Undo should restore original keys
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.editor_state.data.notes[0].key, initial_key_0);
        assert_eq!(editor.editor_state.data.notes[1].key, initial_key_1);
    }
}
