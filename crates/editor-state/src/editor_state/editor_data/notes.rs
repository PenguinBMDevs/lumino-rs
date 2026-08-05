//! 音符操作 —— CRUD、选择框、分割、合并、绘制

use std::collections::HashSet;

use super::super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::super::note_grouping::{self, NoteTuple};
use super::EditorData;
use crate::DragState;
use lumino_note_core::history::OpKind;
use lumino_note_core::note::Note;

impl EditorData {
    /// 同步 notes 到 track_notes 缓存
    pub fn sync_track_notes(&mut self) {
        if self.notes.is_empty() {
            self.track_notes.remove(&self.current_track);
        } else {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
        }
        self.mark_current_track_changed();
    }

    /// 流式同步指定索引的音符到当前 track_notes 缓存。
    ///
    /// 仅复制 `indices` 中的音符，避免整轨克隆。若当前 track 的缓存不存在，
    /// 则回退为完整克隆（一次性初始化）。
    pub fn sync_track_notes_at_indices(&mut self, indices: &[usize]) {
        let current_track = self.current_track;
        if let Some(track_notes) = self.track_notes.get_mut(&current_track) {
            for &note_idx in indices {
                if let Some(src) = self.notes.get(note_idx)
                    && let Some(dst) = track_notes.get_mut(note_idx)
                {
                    dst.clone_from(src);
                }
            }
        } else if !self.notes.is_empty() {
            // 缓存不存在时回退为完整克隆（仅在首次需要同步时发生）
            self.track_notes.insert(current_track, self.notes.clone());
        }
        self.mark_current_track_changed();
    }

    /// 流式应用拖动状态到当前音轨。
    ///
    /// 只修改 `drag_state` 选中的音符，并同步更新 `track_notes` 缓存，
    /// 避免 `apply_to_notes` + `sync_track_notes` 带来的整轨克隆。
    /// 返回实际被修改的音符数。
    ///
    /// **热路径优化**：当 NoteStore 启用时（音符数 ≥ NOTE_STORE_THRESHOLD），
    /// 走 `batch_move_parallel` 8 线程并行路径，16M 全选 20ms（vs 单线程 3.3s）。
    pub fn apply_drag_state_streaming(&mut self, drag_state: &DragState, max_key: u16) -> usize {
        if drag_state.is_delta_zero() {
            return 0;
        }

        // 热路径：NoteStore 启用时走并行批量移动
        if self.note_store_enabled {
            return self.batch_move_notes_from_drag_state(drag_state, max_key);
        }

        let current_track = self.current_track;
        // 缓存不存在时先建立完整快照，后续修改再流式同步。
        if !self.track_notes.contains_key(&current_track) && !self.notes.is_empty() {
            self.track_notes.insert(current_track, self.notes.clone());
        }

        let mut modified = 0usize;
        // 记录实际被修改的索引（主音轨增量事件：等长 UpdateRange）
        let mut modified_indices: Vec<usize> = Vec::new();
        for (note_idx, selected) in drag_state.selected.iter().enumerate() {
            if !selected || note_idx >= self.notes.len() {
                continue;
            }
            if let Some(note) = self.notes.get_mut(note_idx)
                && drag_state.apply_to_note(note, max_key)
            {
                modified += 1;
                modified_indices.push(note_idx);
            }
            if let Some(track_notes) = self.track_notes.get_mut(&current_track)
                && let Some(note) = track_notes.get_mut(note_idx)
            {
                drag_state.apply_to_note(note, max_key);
            }
        }

        if modified > 0 {
            // 增量对账：记录事件 + 流式同步（内部 mark 置 dirty 后清除）
            self.record_update_ranges_streamed(&modified_indices);
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
    ///
    /// 使用 `retain()` O(N) 单次遍历替代逐个 `remove(i)` O(K·log N)，
    /// 1600W 选中音符场景下从 ~56s 降至 ~ms 级。
    ///
    /// **热路径优化**：当 NoteStore 启用时走 `delete_selected`（墓碑标记 + 块级并行）。
    pub fn delete_selected_notes(&mut self, selected: &HashSet<usize>) {
        if selected.is_empty() {
            return;
        }
        self.push_history();
        if self.note_store_enabled {
            self.batch_delete_notes_from_set(selected);
        } else {
            let mut idx = 0usize;
            self.notes.retain(|_| {
                let keep = !selected.contains(&idx);
                idx += 1;
                keep
            });
            self.sync_track_notes();
        }
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
            let note = &self.notes[index];
            if split_tick <= note.tick || split_tick >= note.tick + note.length {
                return false;
            }
            (
                note.tick,
                note.length,
                note.key,
                note.velocity,
                note.channel,
            )
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
            .filter_map(|&note_idx| {
                self.notes.get(note_idx).map(|note| {
                    (
                        note_idx,
                        note.tick,
                        note.key,
                        note.length,
                        note.velocity,
                        note.channel,
                    )
                })
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
            let rm: Vec<usize> = group.iter().map(|note_tuple| note_tuple.0).collect();
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
            .filter_map(|&note_idx| self.notes.get(note_idx).map(|note| (note_idx, note.tick)))
            .collect();

        if selected_notes.len() < 2 {
            return 0;
        }

        // 按 tick 排序（支持不同 Key 混排）
        selected_notes.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 按相同 tick 分组：同一 tick 的所有音符视为一个"和弦/层"，
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

        for group_idx in 0..groups.len() - 1 {
            let current_tick = groups[group_idx].0;
            let next_tick = groups[group_idx + 1].0;
            let new_length = next_tick - current_tick;

            // 当前 tick 组的所有音符都延长到下一组开头
            for &idx in &groups[group_idx].1 {
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
    ///
    /// **热路径优化**：当 NoteStore 启用时，用 `push_note` 同步插入 note_store，
    /// 避免后续 `sync_note_store()` 全量重建。
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
        self.push_note(note.clone());
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
        let mut results = Vec::new();
        for (note_idx, note) in self.notes.iter().enumerate() {
            let ne = note.tick + note.length;
            if note.key >= km && note.key <= kx && note.tick <= te && ne >= ts {
                results.push(note_idx);
            }
        }
        results
    }
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
