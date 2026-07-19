//! 音符操作 —— CRUD、选择框、分割、合并、绘制

use std::collections::HashSet;

use super::super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::super::note_grouping::{self, NoteTuple};
use super::EditorData;
use crate::DragState;
use crate::history::OpKind;
use crate::note::Note;

impl EditorData {
    /// 同步 notes 到 track_notes 缓存
    pub fn sync_track_notes(&mut self) {
        if self.notes.is_empty() {
            self.track_notes.remove(&self.current_track);
        } else {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
        }
        self.mark_track_notes_changed();
    }

    /// 流式同步指定索引的音符到当前 track_notes 缓存。
    ///
    /// 仅复制 `indices` 中的音符，避免整轨克隆。若当前 track 的缓存不存在，
    /// 则回退为完整克隆（一次性初始化）。
    pub fn sync_track_notes_at_indices(&mut self, indices: &[usize]) {
        let current_track = self.current_track;
        if let Some(track_notes) = self.track_notes.get_mut(&current_track) {
            for &i in indices {
                if let Some(src) = self.notes.get(i) {
                    if let Some(dst) = track_notes.get_mut(i) {
                        dst.clone_from(src);
                    }
                }
            }
        } else if !self.notes.is_empty() {
            // 缓存不存在时回退为完整克隆（仅在首次需要同步时发生）
            self.track_notes.insert(current_track, self.notes.clone());
        }
        self.mark_track_notes_changed();
    }

    /// 流式应用拖动状态到当前音轨。
    ///
    /// 只修改 `drag_state` 选中的音符，并同步更新 `track_notes` 缓存，
    /// 避免 `apply_to_notes` + `sync_track_notes` 带来的整轨克隆。
    /// 返回实际被修改的音符数。
    pub fn apply_drag_state_streaming(&mut self, drag_state: &DragState, max_key: u16) -> usize {
        if drag_state.is_delta_zero() {
            return 0;
        }

        let current_track = self.current_track;
        // 缓存不存在时先建立完整快照，后续修改再流式同步。
        if !self.track_notes.contains_key(&current_track) && !self.notes.is_empty() {
            self.track_notes.insert(current_track, self.notes.clone());
        }

        let mut modified = 0usize;
        for (i, selected) in drag_state.selected.iter().enumerate() {
            if !selected || i >= self.notes.len() {
                continue;
            }
            if let Some(note) = self.notes.get_mut(i) {
                if drag_state.apply_to_note(note, max_key) {
                    modified += 1;
                }
            }
            if let Some(track_notes) = self.track_notes.get_mut(&current_track) {
                if let Some(note) = track_notes.get_mut(i) {
                    drag_state.apply_to_note(note, max_key);
                }
            }
        }

        if modified > 0 {
            self.mark_track_notes_changed();
        }
        modified
    }

    /// 通过索引删除单个音符
    pub fn delete_note_by_index(&mut self, index: usize) {
        if index < self.notes.len() {
            self.push_history();
            self.notes.remove(index);
            self.sync_track_notes();
        }
    }

    /// 批量删除选中音符
    pub fn delete_selected_notes(&mut self, selected: &HashSet<usize>) {
        if selected.is_empty() {
            return;
        }
        self.push_history();
        let mut indices: Vec<usize> = selected.iter().copied().collect();
        indices.sort_by(|a, b| b.cmp(a));
        for &i in &indices {
            if i < self.notes.len() {
                self.notes.remove(i);
            }
        }
        self.sync_track_notes();
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> HashSet<usize> {
        (0..self.notes.len()).collect()
    }

    /// 分割音符
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        if index >= self.notes.len() {
            return false;
        }
        let (note_tick, note_length, key, velocity, channel) = {
            let n = &self.notes[index];
            if split_tick <= n.tick || split_tick >= n.tick + n.length {
                return false;
            }
            (n.tick, n.length, n.key, n.velocity, n.channel)
        };
        self.push_history();
        self.notes.remove(index);
        let right = Note::from_raw(
            split_tick,
            key,
            note_tick + note_length - split_tick,
            velocity,
            channel,
        );
        self.notes.insert(index, right);
        let left = Note::from_raw(note_tick, key, split_tick - note_tick, velocity, channel);
        self.notes.insert(index, left);
        self.sync_track_notes();
        true
    }

    /// 合并选中音符
    pub fn glue_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        let selected_notes: Vec<NoteTuple> = sel
            .iter()
            .filter_map(|&i| {
                self.notes
                    .get(i)
                    .map(|n| (i, n.tick, n.key, n.length, n.velocity, n.channel))
            })
            .collect();
        if selected_notes.is_empty() {
            return 0;
        }

        let groups = note_grouping::group_adjacent_notes(&selected_notes, GLUE_PROXIMITY_THRESHOLD);
        if groups.is_empty() {
            return 0;
        }

        self.push_history();
        let mut merged = 0usize;
        for group in &groups {
            let first = &group[0];
            let last = &group[group.len() - 1];
            let merged_tick = first.1;
            let merged_length = (last.1 + last.3) - merged_tick;
            let rm: Vec<usize> = group.iter().map(|n| n.0).collect();
            let mut rm_sorted = rm.clone();
            rm_sorted.sort_by(|a, b| b.cmp(a));
            for &idx in &rm_sorted {
                self.notes.remove(idx);
            }
            let adj = rm[0].min(self.notes.len());
            self.notes.insert(
                adj,
                Note::from_raw(merged_tick, first.2, merged_length, first.4, first.5),
            );
            merged += 1;
        }
        self.sync_track_notes();
        merged
    }

    /// 连奏选中音符：按 tick 排序，填充相邻音符之间的间隙。
    /// 仅在前一个音符的结尾与后一个音符的开始之间有间隙时延长，
    /// 不会缩短重叠的音符。最后一个音符保持不变。
    pub fn tie_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.len() < 2 {
            return 0;
        }

        // 收集选中音符信息 (index, tick)
        let mut selected_notes: Vec<(usize, f32)> = sel
            .iter()
            .filter_map(|&i| self.notes.get(i).map(|n| (i, n.tick)))
            .collect();

        if selected_notes.len() < 2 {
            return 0;
        }

        // 按 tick 排序（支持不同 Key 混排）
        selected_notes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 按相同 tick 分组：同一 tick 的所有音符视为一个“和弦/层”，
        // 统一延长到下一组的 tick
        let mut groups: Vec<(f32, Vec<usize>)> = Vec::new();
        for (idx, tick) in selected_notes {
            match groups.last_mut() {
                Some(last) if last.0 == tick => last.1.push(idx),
                _ => groups.push((tick, vec![idx])),
            }
        }

        if groups.len() < 2 {
            return 0;
        }

        let mut tied = 0usize;
        self.push_history();

        for i in 0..groups.len() - 1 {
            let current_tick = groups[i].0;
            let next_tick = groups[i + 1].0;
            let new_length = next_tick - current_tick;

            // 当前 tick 组的所有音符都延长到下一组开头
            for &idx in &groups[i].1 {
                if let Some(note) = self.notes.get_mut(idx)
                    && new_length > note.length
                {
                    note.length = new_length;
                    tied += 1;
                }
            }
        }

        if tied > 0 {
            self.sync_track_notes();
        }
        tied
    }

    /// 完成绘制新音符（纯业务逻辑），返回创建的 Note
    pub fn finish_drawing(
        &mut self,
        start_tick: f32,
        key: u16,
        current_tick: f32,
        snap_precision: f32,
        default_note_length: f32,
    ) -> Option<Note> {
        if self.current_track == 0 {
            tracing::debug!("编辑器: Conductor 轨道禁止放置音符");
            return None;
        }
        let (tick, length) = if current_tick > start_tick {
            (start_tick, current_tick - start_tick)
        } else if current_tick < start_tick {
            (current_tick, start_tick - current_tick)
        } else {
            (start_tick, default_note_length)
        };
        let length = length.max(snap_precision);
        // 合并窗口：300ms 内连续放置音符合并为一个 NoteCreate 日志，
        // 单条日志超过 entry_limit 条目时自动分割为新日志（parent_group_id 串联）
        let merged = self.push_history_mergeable(OpKind::NoteCreate);
        if merged {
            tracing::debug!("编辑器: 音符放置已合并到当前 NoteCreate 日志");
        }
        let note = Note::new(tick, key, length);
        self.notes.push_back(note.clone());
        self.track_notes
            .insert(self.current_track, self.notes.clone());
        self.mark_track_notes_changed();
        tracing::debug!(
            "编辑器: 已保存 {} 个音符到音轨 {}",
            self.notes.len(),
            self.current_track
        );
        Some(note)
    }

    /// 计算选择框内的音符索引（委托到 get_notes_in_selection_box，消除重复逻辑）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> HashSet<usize> {
        self.get_notes_in_selection_box(start_tick, start_key, current_tick, current_key)
            .into_iter()
            .collect()
    }

    /// 获取选择框内的音符索引列表
    pub fn get_notes_in_selection_box(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> Vec<usize> {
        let ts = start_tick.min(current_tick);
        let te = start_tick.max(current_tick);
        let km = start_key.min(current_key);
        let kx = start_key.max(current_key);
        let mut r = Vec::new();
        for (i, n) in self.notes.iter().enumerate() {
            let ne = n.tick + n.length;
            if n.key >= km && n.key <= kx && n.tick < te && ne > ts {
                r.push(i);
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bit_vec::BitVec;

    #[test]
    fn test_apply_drag_state_streaming_moves_selected_and_syncs_track() {
        let mut data = EditorData::new();
        data.current_track = 1;
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.notes.push_back(Note::new(10.0, 62, 1.0));
        data.notes.push_back(Note::new(20.0, 64, 1.0));
        data.track_notes.insert(1, data.notes.clone());

        let mut bv = BitVec::from_elem(3, false);
        bv.set(0, true);
        bv.set(2, true);
        let drag_state = DragState::new(bv, 0, 60);
        let mut ds = drag_state;
        ds.set_delta(5, -2);

        let modified = data.apply_drag_state_streaming(&ds, 127);
        assert_eq!(modified, 2);

        // notes 已更新
        assert_eq!(data.notes[0].tick, 5.0);
        assert_eq!(data.notes[0].key, 58);
        assert_eq!(data.notes[1].tick, 10.0, "未选中音符不变");
        assert_eq!(data.notes[1].key, 62);
        assert_eq!(data.notes[2].tick, 25.0);
        assert_eq!(data.notes[2].key, 62);

        // track_notes 同步更新
        let track = data.track_notes.get(&1).unwrap();
        assert_eq!(track[0].tick, 5.0);
        assert_eq!(track[0].key, 58);
        assert_eq!(track[1].tick, 10.0);
        assert_eq!(track[2].tick, 25.0);
        assert_eq!(data.track_notes_gen, 1);
    }

    #[test]
    fn test_apply_drag_state_streaming_zero_delta_is_noop() {
        let mut data = EditorData::new();
        data.current_track = 1;
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.track_notes.insert(1, data.notes.clone());

        let ds = DragState::from_single(0, 1, 0, 60);
        let modified = data.apply_drag_state_streaming(&ds, 127);
        assert_eq!(modified, 0);
        assert_eq!(data.track_notes_gen, 0, "无变更时不应 bump 版本");
    }

    #[test]
    fn test_sync_track_notes_at_indices_partial() {
        let mut data = EditorData::new();
        data.current_track = 2;
        data.notes.push_back(Note::new(0.0, 60, 1.0));
        data.notes.push_back(Note::new(10.0, 62, 1.0));
        data.notes.push_back(Note::new(20.0, 64, 1.0));
        data.track_notes.insert(2, data.notes.clone());

        // 只改 notes[1]
        data.notes[1].tick = 99.0;
        data.notes[1].key = 70;

        data.sync_track_notes_at_indices(&[1]);

        let track = data.track_notes.get(&2).unwrap();
        assert_eq!(track[0].tick, 0.0, "未同步索引保持不变");
        assert_eq!(track[1].tick, 99.0, "同步索引已更新");
        assert_eq!(track[1].key, 70);
        assert_eq!(track[2].tick, 20.0, "未同步索引保持不变");
        assert_eq!(data.track_notes_gen, 1);
    }

    #[test]
    fn test_sync_track_notes_at_indices_creates_entry_when_missing() {
        let mut data = EditorData::new();
        data.current_track = 3;
        data.notes.push_back(Note::new(5.0, 60, 2.0));

        data.sync_track_notes_at_indices(&[0]);

        let track = data.track_notes.get(&3).unwrap();
        assert_eq!(track[0].tick, 5.0);
        assert_eq!(data.track_notes_gen, 1);
    }
}
