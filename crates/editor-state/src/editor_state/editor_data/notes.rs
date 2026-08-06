//! 音符操作 —— CRUD、选择框、分割、合并、绘制
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `document`（MidiDocument），
//! 本模块所有操作直接读写当前音轨（`track_notes_mut` / 写访问器），
//! 不再维护 `notes` / `track_notes` 冗余副本。

use std::collections::HashSet;

use super::super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::super::note_grouping::{self, NoteTuple};
use super::EditorData;
use crate::DragState;
use lumino_note_core::history::CreateOp;
use lumino_note_core::note::Note;

impl EditorData {
    /// 流式应用拖动状态到当前音轨。
    ///
    /// 只修改 `drag_state` 选中的音符（直接改 document 当前轨）。
    /// 返回实际被修改的音符数。
    pub fn apply_drag_state_streaming(&mut self, drag_state: &DragState, max_key: u16) -> usize {
        if drag_state.is_delta_zero() {
            return 0;
        }

        let mut modified = 0usize;
        // 记录实际被修改的索引（主音轨增量事件：等长 UpdateRange）
        let mut modified_indices: Vec<usize> = Vec::new();

        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for (note_idx, selected) in drag_state.selected.iter().enumerate() {
                if !selected || note_idx >= track.len() {
                    continue;
                }
                if let Some(note) = track.get_mut(note_idx) {
                    let mut note_f = super::accessors::event_to_note(note);
                    if drag_state.apply_to_note(&mut note_f, max_key) {
                        note.start_tick = super::accessors::f32_to_tick(note_f.tick);
                        note.end_tick = note.end_tick.max(note.start_tick.saturating_add(1));
                        note.key = note_f.key as u8;
                        modified += 1;
                        modified_indices.push(note_idx);
                    }
                }
            }
        }

        if modified > 0 {
            // 增量对账：记录事件（内部 mark 置 dirty 后清除）
            self.record_update_ranges_streamed(&modified_indices);
        }
        modified
    }

    /// 通过索引删除单个音符（直接操作 document 当前轨）
    pub fn delete_note_by_index(&mut self, index: usize) {
        if self.remove_note(self.current_track, index).is_some() {
            self.push_history();
            self.mark_current_track_changed();
        }
    }

    /// 批量删除选中音符
    ///
    /// 索引降序逐个删除，避免索引漂移；相比 retain 的 O(N) 有 K 次删除开销，
    /// 但 document 为唯一权威源，语义清晰。第二阶段分块后删除走块级批量路径。
    pub fn delete_selected_notes(&mut self, selected: &HashSet<usize>) {
        if selected.is_empty() {
            return;
        }
        self.push_history();
        let mut sorted: Vec<usize> = selected.iter().copied().collect();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let mut deleted = 0usize;
        for idx in sorted {
            if self.remove_note(self.current_track, idx).is_some() {
                deleted += 1;
            }
        }
        if deleted > 0 {
            self.mark_current_track_changed();
        }
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> HashSet<usize> {
        (0..self.current_track_note_count()).collect()
    }

    /// 分割音符
    pub fn split_note(&mut self, index: usize, split_tick: f32) -> bool {
        let track = self.current_track_notes();
        let Some(note) = track.get(index) else {
            return false;
        };
        let note_tick = note.start_tick as f32;
        let note_length = (note.end_tick - note.start_tick) as f32;
        if split_tick <= note_tick || split_tick >= note_tick + note_length {
            return false;
        }
        let (key, velocity, channel) = (note.key, note.velocity, note.channel);

        self.push_history();
        // 移除原音符，插入 right + left（insert_note 按 start_tick 有序插入）
        self.remove_note(self.current_track, index);
        let right = Note::from_raw(
            split_tick,
            key as u16,
            note_tick + note_length - split_tick,
            velocity,
            channel,
        );
        let left = Note::from_raw(
            note_tick,
            key as u16,
            split_tick - note_tick,
            velocity,
            channel,
        );
        self.insert_note(self.current_track, left);
        self.insert_note(self.current_track, right);
        self.mark_current_track_changed();
        true
    }

    /// 合并选中音符
    pub fn glue_selected_notes(&mut self, selected: &HashSet<usize>) -> usize {
        let sel: Vec<usize> = selected.iter().copied().collect();
        if sel.is_empty() {
            return 0;
        }
        let track = self.current_track_notes();
        let selected_notes: Vec<NoteTuple> = sel
            .iter()
            .filter_map(|&note_idx| {
                track.get(note_idx).map(|note| {
                    (
                        note_idx,
                        note.start_tick as f32,
                        note.key as u16,
                        (note.end_tick - note.start_tick) as f32,
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
                self.remove_note(self.current_track, idx);
            }
            let merged_note = Note::from_raw(merged_tick, first.2, merged_length, first.4, first.5);
            self.insert_note(self.current_track, merged_note);
            merged += 1;
        }
        self.mark_current_track_changed();
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

        let track = self.current_track_notes();
        // 收集选中音符信息 (index, tick)
        let mut selected_notes: Vec<(usize, f32)> = sel
            .iter()
            .filter_map(|&note_idx| {
                track
                    .get(note_idx)
                    .map(|note| (note_idx, note.start_tick as f32))
            })
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

        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for group_idx in 0..groups.len() - 1 {
                let current_tick = groups[group_idx].0;
                let next_tick = groups[group_idx + 1].0;
                let new_length = next_tick - current_tick;

                // 当前 tick 组的所有音符都延长到下一组开头
                for &idx in &groups[group_idx].1 {
                    if let Some(note) = track.get_mut(idx) {
                        let current_length = (note.end_tick - note.start_tick) as f32;
                        if new_length > current_length {
                            note.end_tick = note.start_tick + new_length as u32;
                            tied += 1;
                        }
                    }
                }
            }
        }

        if tied > 0 {
            self.mark_current_track_changed();
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
        let note = Note::new(tick, key, length);
        if self.insert_note(self.current_track, note.clone()) {
            // 增量、极简操作日志：每 op 记录单个音符（20 字节）替代整轨快照克隆。
            // 合并窗口语义不变（300ms 内连续放置合并为一条 CreateEntry），
            // 但 undo/redo 恢复是 O(op 数) 精确位置操作，与音符总量解耦——
            // 1600W 音符工程铅笔绘制不再触发整轨快照（原 `push_history_mergeable` 路径
            // 每条都 `..top.clone()` 复制整个 EditorSnapshot）。
            let op = CreateOp {
                track_id: self.current_track as u32,
                note: super::accessors::note_to_event(note.clone()),
            };
            let merged = self.push_note_create(vec![op]);
            if merged {
                tracing::debug!("编辑器: 音符放置已合并到当前 NoteCreate 日志");
            }
        }
        self.mark_current_track_changed();
        tracing::debug!("编辑器: 已保存 1 个音符到音轨 {}", self.current_track);
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
        for (note_idx, note) in self.current_track_notes().iter().enumerate() {
            let tick = note.start_tick as f32;
            let ne = note.end_tick as f32;
            if note.key as u16 >= km && note.key as u16 <= kx && tick <= te && ne >= ts {
                results.push(note_idx);
            }
        }
        results
    }
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
