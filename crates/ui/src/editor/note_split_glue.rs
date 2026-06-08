//! 音符分割与合并操作模块
//!
//! 实现 Razor 工具的分割功能和 Glue 合并功能。

use super::Editor;

/// 内部辅助结构：一组待合并音符的信息
struct NoteInfo {
    tick: f32,
    key: u16,
    length: f32,
    velocity: u8,
    channel: u8,
    remove_indices: Vec<usize>,
}

impl Editor {
    /// 在 tick 位置分割音符
    ///
    /// # 参数
    /// * `index` - 音符在 notes 中的索引
    /// * `split_tick` - 分割位置的 tick（已吸附到 Snap 精度）
    ///
    /// # 返回值
    /// 分割是否成功
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        let note_len = self.editor_state.data.notes.len();
        if index >= note_len {
            return false;
        }

        // 先复制音符数据，避免借用冲突
        let (note_tick, note_length, key, velocity, channel) = {
            let n = &self.editor_state.data.notes[index];
            if split_tick <= n.tick || split_tick >= n.tick + n.length {
                return false;
            }
            (n.tick, n.length, n.key, n.velocity, n.channel)
        };

        let left_tick = note_tick;
        let left_length = split_tick - note_tick;
        let right_tick = split_tick;
        let right_length = note_tick + note_length - split_tick;

        // 推入历史记录
        self.push_history();

        // 移除原音符
        self.editor_state.data.notes.remove(index);

        // 插入右侧音符（先插入，这样右侧在 index 位置）
        let right_note = super::Note::from_raw(right_tick, key, right_length, velocity, channel);
        self.editor_state.data.notes.insert(index, right_note);

        // 插入左侧音符
        let left_note = super::Note::from_raw(left_tick, key, left_length, velocity, channel);
        self.editor_state.data.notes.insert(index, left_note);

        // 更新选中状态和悬停状态
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;

        self.mark_notes_changed();
        true
    }

    /// 合并选中音符中同 key 的相邻音符
    ///
    /// # 返回值
    /// 实际合并的音符组数（每组消耗 1+ 个音符）
    pub fn glue_selected_notes(&mut self) -> usize {
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

        // 收集选中音符的信息并排序
        let mut selected_notes: Vec<(usize, f32, u16, f32, u8, u8)> = selected
            .iter()
            .filter_map(|&i| {
                notes
                    .get(i)
                    .map(|n| (i, n.tick, n.key, n.length, n.velocity, n.channel))
            })
            .collect();

        if selected_notes.is_empty() {
            return 0;
        }

        // 按 tick 排序
        selected_notes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 分组：同 key 且 tick 连续或重叠
        let mut groups: Vec<Vec<(usize, f32, u16, f32, u8, u8)>> = Vec::new();
        for note in &selected_notes {
            let added = if let Some(last_group) = groups.last_mut() {
                let last_note = last_group.last().unwrap();
                if last_note.2 == note.2 {
                    // 同 key，检查是否相邻或重叠
                    let last_end = last_note.1 + last_note.3;
                    if note.1 <= last_end + 1.0 {
                        // 相邻或重叠，加入当前组
                        last_group.push(*note);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            };

            if !added {
                groups.push(vec![*note]);
            }
        }

        // 过滤出真正需要合并的组（至少 2 个音符）
        let groups_to_merge: Vec<Vec<(usize, f32, u16, f32, u8, u8)>> =
            groups.into_iter().filter(|g| g.len() >= 2).collect();

        if groups_to_merge.is_empty() {
            return 0;
        }

        // 推入历史记录
        self.push_history();

        let mut merged_count = 0usize;

        // 需要反向处理组（从右到左的索引），避免索引偏移
        // 先收集所有要删除的索引和要插入的音符
        let mut all_ops: Vec<(usize, NoteInfo)> = Vec::new();

        for group in &groups_to_merge {
            let first = &group[0];
            let last = &group[group.len() - 1];

            let merged_tick = first.1;
            let merged_key = first.2;
            let merged_velocity = first.4;
            let merged_channel = first.5;
            let merged_length = (last.1 + last.3) - merged_tick;

            let insert_idx = group[0].0;
            let remove_indices: Vec<usize> = group.iter().map(|n| n.0).collect();

            all_ops.push((
                insert_idx,
                NoteInfo {
                    tick: merged_tick,
                    key: merged_key,
                    length: merged_length,
                    velocity: merged_velocity,
                    channel: merged_channel,
                    remove_indices,
                },
            ));
        }

        // 按 insert_idx 从大到小排序，这样后面的操作不会影响前面的索引
        all_ops.sort_by(|a, b| b.0.cmp(&a.0));

        for (_insert_idx, info) in &all_ops {
            // 删除音符（从大到小）
            let mut rm = info.remove_indices.clone();
            rm.sort_by(|a, b| b.cmp(a));
            for &idx in &rm {
                self.editor_state.data.notes.remove(idx);
            }

            // 因为是从大到小删除，insert_idx 可能需要调整
            // 在反向排序的处理中，insert_idx 之前没有被删除的音符，所以可以直接用
            let adjusted_idx = info.remove_indices[0].min(self.editor_state.data.notes.len());

            // 插入合并后的音符
            let merged_note = super::Note::from_raw(
                info.tick,
                info.key,
                info.length,
                info.velocity,
                info.channel,
            );
            self.editor_state
                .data
                .notes
                .insert(adjusted_idx, merged_note);

            merged_count += 1;
        }

        // 清除选中和悬停状态
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;

        self.mark_notes_changed();
        merged_count
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

    // ========== 分割测试 ==========

    #[test]
    fn test_split_note_basic() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 480.0)]);

        let result = editor.split_note(0, 240.0);

        assert!(result, "分割应成功");
        assert_eq!(editor.editor_state.data.notes.len(), 2);
        // 左侧: tick=0, length=240
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 240.0);
        assert_eq!(editor.editor_state.data.notes[0].key, 60);
        // 右侧: tick=240, length=240
        assert_eq!(editor.editor_state.data.notes[1].tick, 240.0);
        assert_eq!(editor.editor_state.data.notes[1].length, 240.0);
        assert_eq!(editor.editor_state.data.notes[1].key, 60);
    }

    #[test]
    fn test_split_note_outside_bounds() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 480.0)]);

        // 分割点在起始位置 → 失败
        assert!(!editor.split_note(0, 0.0));
        // 分割点在结束位置 → 失败
        assert!(!editor.split_note(0, 480.0));
        // 分割点在外部 → 失败
        assert!(!editor.split_note(0, 1000.0));
        // 无效索引 → 失败
        assert!(!editor.split_note(5, 240.0));

        // 音符数量不变
        assert_eq!(editor.editor_state.data.notes.len(), 1);
    }

    #[test]
    fn test_split_note_preserves_properties() {
        let mut editor =
            create_test_editor_with_notes(vec![Note::from_raw(100.0, 72, 480.0, 100, 1)]);

        let result = editor.split_note(0, 340.0);
        assert!(result);

        // 检查属性保留
        assert_eq!(editor.editor_state.data.notes[0].key, 72);
        assert_eq!(editor.editor_state.data.notes[0].velocity, 100);
        assert_eq!(editor.editor_state.data.notes[0].channel, 1);
        assert_eq!(editor.editor_state.data.notes[1].key, 72);
        assert_eq!(editor.editor_state.data.notes[1].velocity, 100);
        assert_eq!(editor.editor_state.data.notes[1].channel, 1);
    }

    #[test]
    fn test_split_note_undo() {
        let mut editor = create_test_editor_with_notes(vec![Note::new(0.0, 60, 480.0)]);

        let result = editor.split_note(0, 240.0);
        assert!(result);
        assert_eq!(editor.editor_state.data.notes.len(), 2);

        // 撤销
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.editor_state.data.notes.len(), 1);
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 480.0);
    }

    // ========== 合并测试 ==========

    #[test]
    fn test_glue_two_same_key_notes() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 60, 240.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();

        assert_eq!(merged, 1);
        assert_eq!(editor.editor_state.data.notes.len(), 1);
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 480.0);
        assert_eq!(editor.editor_state.data.notes[0].key, 60);
    }

    #[test]
    fn test_glue_three_notes() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 120.0),
            Note::new(120.0, 60, 120.0),
            Note::new(240.0, 60, 240.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();

        assert_eq!(merged, 1);
        assert_eq!(editor.editor_state.data.notes.len(), 1);
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 480.0);
    }

    #[test]
    fn test_glue_different_keys_not_merged() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 64, 240.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();

        // 不同 key 不合并
        assert_eq!(merged, 0);
        assert_eq!(editor.editor_state.data.notes.len(), 2);
    }

    #[test]
    fn test_glue_no_selection() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 60, 240.0),
        ]);

        let merged = editor.glue_selected_notes();
        assert_eq!(merged, 0);
    }

    #[test]
    fn test_glue_with_gap_stops_at_gap() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 120.0),
            Note::new(120.0, 60, 120.0),
            // 这里有间隙
            Note::new(360.0, 60, 120.0),
            Note::new(480.0, 60, 120.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();

        // 合并两组：{0,1} 和 {2,3}
        assert_eq!(merged, 2);
        assert_eq!(editor.editor_state.data.notes.len(), 2);
        // 第一组: tick=0, length=240
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 240.0);
        // 第二组: tick=360, length=240
        assert_eq!(editor.editor_state.data.notes[1].tick, 360.0);
        assert_eq!(editor.editor_state.data.notes[1].length, 240.0);
    }

    #[test]
    fn test_glue_overlapping_notes() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 300.0),
            Note::new(100.0, 60, 300.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();

        assert_eq!(merged, 1);
        assert_eq!(editor.editor_state.data.notes.len(), 1);
        // 合并: tick=0, length=400-0=400
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 400.0);
    }

    #[test]
    fn test_glue_undo() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 60, 240.0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();
        assert_eq!(merged, 1);

        // 撤销
        let undone = editor.undo();
        assert!(undone);
        assert_eq!(editor.editor_state.data.notes.len(), 2);
        assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
        assert_eq!(editor.editor_state.data.notes[0].length, 240.0);
        assert_eq!(editor.editor_state.data.notes[1].tick, 240.0);
        assert_eq!(editor.editor_state.data.notes[1].length, 240.0);
    }

    #[test]
    fn test_glue_velocity_from_first_note() {
        let mut editor = create_test_editor_with_notes(vec![
            Note::from_raw(0.0, 60, 120.0, 100, 0),
            Note::from_raw(120.0, 60, 120.0, 80, 0),
        ]);
        select_all_notes(&mut editor);

        let merged = editor.glue_selected_notes();
        assert_eq!(merged, 1);

        // velocity 使用第一个音符的值
        assert_eq!(editor.editor_state.data.notes[0].velocity, 100);
    }
}
