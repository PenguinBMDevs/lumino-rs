//! 音符操作 —— CRUD、选择框、分割、合并、绘制

use std::collections::HashSet;

use super::super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::super::note_grouping::{self, NoteTuple};
use super::EditorData;
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

    /// 连奏选中音符：按 tick 排序，前一个音符延长到后一个音符的开始位置。
    /// 支持不同 Key 的音符连奏，不要求同 Key 分组。最后一个音符保持不变。
    pub fn tie_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.len() < 2 {
            return 0;
        }

        // 收集选中音符信息 (index, tick, length)
        let mut selected_notes: Vec<(usize, f32, f32)> = sel
            .iter()
            .filter_map(|&i| self.notes.get(i).map(|n| (i, n.tick, n.length)))
            .collect();

        if selected_notes.len() < 2 {
            return 0;
        }

        // 按 tick 排序（支持不同 Key 混排）
        selected_notes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut tied = 0usize;
        self.push_history();

        for i in 0..selected_notes.len() - 1 {
            let prev_idx = selected_notes[i].0;
            let prev_tick = selected_notes[i].1;
            let curr_tick = selected_notes[i + 1].1;

            if curr_tick > prev_tick {
                let new_length = curr_tick - prev_tick;
                if let Some(note) = self.notes.get_mut(prev_idx) {
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
        self.push_history();
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
