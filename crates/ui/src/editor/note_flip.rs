//! 音符翻转操作模块

use super::Editor;
use crate::toolbar::FlipHorizontalMode;

impl Editor {
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

    /// 水平翻转选中音符
    ///
    /// 根据指定的翻转模式，沿水平方向镜像翻转选中音符的位置。
    /// 三种模式：
    /// - Center: 沿选中音符的tick范围中心翻转
    /// - Left: 沿最左侧边缘（最小tick）翻转
    /// - Right: 沿最右侧边缘（最大tick）翻转
    ///
    /// # 参数
    /// * `mode` - 翻转模式
    ///
    /// # 返回值
    /// 实际修改的音符数量
    pub fn flip_selected_notes_horizontal(&mut self, mode: FlipHorizontalMode) -> usize {
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

        // 计算选中音符的tick范围
        let mut min_tick = f32::INFINITY;
        let mut max_tick_end = f32::NEG_INFINITY;
        let mut has_valid = false;

        for &i in &selected {
            if let Some(note) = notes.get(i) {
                min_tick = min_tick.min(note.tick);
                max_tick_end = max_tick_end.max(note.tick + note.length);
                has_valid = true;
            }
        }

        if !has_valid {
            return 0;
        }

        // 根据模式计算翻转轴
        let axis_tick = match mode {
            FlipHorizontalMode::Center => (min_tick + max_tick_end) / 2.0,
            FlipHorizontalMode::Left => min_tick,
            FlipHorizontalMode::Right => max_tick_end,
        };

        // 推入历史记录
        self.push_history();

        let mut modified = 0usize;

        for &i in &selected {
            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                // 翻转音符的起始位置：以axis_tick为轴镜像
                let new_tick = 2.0 * axis_tick - (note.tick + note.length);
                let new_tick = new_tick.max(0.0); // 不能小于0

                if (new_tick - note.tick).abs() > f32::EPSILON {
                    note.tick = new_tick;
                    modified += 1;
                }
            }
        }

        if modified > 0 {
            self.mark_notes_changed();
        } else {
            // 没有实际修改（例如所有音符都在轴上），回退历史记录
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
    use super::*;
    use crate::editor::note::Note;

    fn create_test_editor_with_notes(notes: Vec<Note>) -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.data.notes = notes.into();
        editor
    }

    // ========== 垂直翻转测试 ==========

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

    // ========== 水平翻转测试 ==========

    #[test]
    fn test_flip_horizontal_center() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),   // tick=0, length=100
            Note::new(200.0, 64, 100.0), // tick=200, length=100
        ]);
        editor.select_all_notes();

        // tick范围: 0..300, 中心=150
        // 音符0: new_tick = 2*150 - (0+100) = 200
        // 音符1: new_tick = 2*150 - (200+100) = 0
        let modified = editor.flip_selected_notes_horizontal(FlipHorizontalMode::Center);

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].tick, 200.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 0.0);
        // 长度不变
        assert_eq!(editor.editor_state.data.notes[0].length, 100.0);
        assert_eq!(editor.editor_state.data.notes[1].length, 100.0);
    }

    #[test]
    fn test_flip_horizontal_left() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(100.0, 60, 50.0), // tick=100, length=50
            Note::new(200.0, 64, 75.0), // tick=200, length=75
        ]);
        editor.select_all_notes();

        // 最左边缘: min_tick = 100
        // 音符0: new_tick = 2*100 - (100+50) = 50
        // 音符1: new_tick = 2*100 - (200+75) = -75 -> clamp to 0
        let modified = editor.flip_selected_notes_horizontal(FlipHorizontalMode::Left);

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].tick, 50.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 0.0);
    }

    #[test]
    fn test_flip_horizontal_right() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(100.0, 60, 50.0), // tick=100, length=50, end=150
            Note::new(200.0, 64, 75.0), // tick=200, length=75, end=275
        ]);
        editor.select_all_notes();

        // 最右边缘: max_tick_end = 275
        // 音符0: new_tick = 2*275 - (100+50) = 400
        // 音符1: new_tick = 2*275 - (200+75) = 275
        let modified = editor.flip_selected_notes_horizontal(FlipHorizontalMode::Right);

        assert_eq!(modified, 2);
        assert_eq!(editor.editor_state.data.notes[0].tick, 400.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 275.0);
    }

    #[test]
    fn test_flip_horizontal_no_selection() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(100.0, 60, 50.0)]);

        let modified = editor.flip_selected_notes_horizontal(FlipHorizontalMode::Center);

        assert_eq!(modified, 0);
        assert_eq!(editor.editor_state.data.notes[0].tick, 100.0);
    }

    #[test]
    fn test_flip_horizontal_preserves_key_and_length() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(100.0, 60, 50.0),
            Note::new(200.0, 64, 75.0),
        ]);
        editor.select_all_notes();

        editor.flip_selected_notes_horizontal(FlipHorizontalMode::Center);

        assert_eq!(editor.editor_state.data.notes[0].key, 60);
        assert_eq!(editor.editor_state.data.notes[0].length, 50.0);
        assert_eq!(editor.editor_state.data.notes[1].key, 64);
        assert_eq!(editor.editor_state.data.notes[1].length, 75.0);
    }

    #[test]
    fn test_flip_horizontal_history() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 100.0),
            Note::new(200.0, 64, 100.0),
        ]);
        editor.select_all_notes();

        let initial_tick_0 = editor.editor_state.data.notes[0].tick;
        let initial_tick_1 = editor.editor_state.data.notes[1].tick;

        editor.flip_selected_notes_horizontal(FlipHorizontalMode::Center);
        assert_eq!(editor.editor_state.data.notes[0].tick, 200.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 0.0);

        // Undo should restore original ticks
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.editor_state.data.notes[0].tick, initial_tick_0);
        assert_eq!(editor.editor_state.data.notes[1].tick, initial_tick_1);
    }
}
