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
}

// 测试已存在于 editor/tests.rs 的 speed_tests 模块中
