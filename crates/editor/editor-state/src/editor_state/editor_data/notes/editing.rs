//! 音符编辑：分割、合并、连奏与删除增量事件合并（自 `notes.rs` 拆分，保持各文件 < 400 行）

use super::super::super::constants::GLUE_PROXIMITY_THRESHOLD;
use super::super::super::note_grouping::{self, NoteTuple};
use super::super::super::selection_set::SelectionSet;
use super::super::{CollabTransformSyncEntry, EditorData};
use lumino_note_core::note::Note;

impl EditorData {
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
        let track_idx = self.current_track;

        self.push_history();
        // 移除原音符，插入 right + left（insert_note 按 start_tick 有序插入，按值）
        self.remove_note(track_idx, index);
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
        let _ = self.insert_note_with_id(track_idx, left);
        let _ = self.insert_note_with_id(track_idx, right);
        self.mark_current_track_changed();
        // 2026-09 去 ID 协作修复：分割改变音符数量，须广播「删原 + 加左右」让 B 端同步（按值）。
        // 协作同步关闭时跳过对账（消费端 `is_connected` 会短路丢弃）。
        if self.collab_sync_enabled {
            self.pending_collab_transform_sync.push((
                false,
                note_tick,
                key as u16,
                note_length,
                velocity,
                channel,
                track_idx,
            ));
            self.pending_collab_transform_sync.push((
                true,
                note_tick,
                key as u16,
                split_tick - note_tick,
                velocity,
                channel,
                track_idx,
            ));
            self.pending_collab_transform_sync.push((
                true,
                split_tick,
                key as u16,
                note_tick + note_length - split_tick,
                velocity,
                channel,
                track_idx,
            ));
        }
        true
    }

    /// 合并选中音符
    pub fn glue_selected_notes(&mut self, selected: &SelectionSet) -> usize {
        let sel: Vec<usize> = selected.iter().collect();
        if sel.is_empty() {
            return 0;
        }
        let track = self.current_track_notes();
        let mut selected_notes: Vec<NoteTuple> = Vec::with_capacity(sel.len());
        for &note_idx in &sel {
            if let Some(note) = track.get(note_idx) {
                selected_notes.push((
                    note_idx,
                    note.start_tick as f32,
                    note.key as u16,
                    (note.end_tick - note.start_tick) as f32,
                    note.velocity,
                    note.channel,
                ));
            }
        }
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
            // 2026-09 去 ID 协作修复：合并改变音符数量，先记录每个被合并音符的删除（按值）。
            if self.collab_sync_enabled {
                for nt in group {
                    self.pending_collab_transform_sync.push((
                        false,
                        nt.1,
                        nt.2,
                        nt.3,
                        nt.4,
                        nt.5,
                        self.current_track,
                    ));
                }
            }
            for &idx in &rm_sorted {
                self.remove_note(self.current_track, idx);
            }
            let merged_note = Note::from_raw(merged_tick, first.2, merged_length, first.4, first.5);
            let _ = self.insert_note_with_id(self.current_track, merged_note);
            // 2026-09 去 ID 协作修复：添加一个合并后的音符（按值）。
            if self.collab_sync_enabled {
                self.pending_collab_transform_sync.push((
                    true,
                    merged_tick,
                    first.2,
                    merged_length,
                    first.4,
                    first.5,
                    self.current_track,
                ));
            }
            merged += 1;
        }
        self.mark_current_track_changed();
        merged
    }

    /// 连奏选中音符：按 tick 排序，填充相邻音符之间的间隙。
    /// 仅在前一个音符的结尾与后一个音符的开始之间有间隙时延长，
    /// 不会缩短重叠的音符。最后一个音符保持不变。
    pub fn tie_selected_notes(&mut self, selected: &SelectionSet) -> usize {
        let sel: Vec<usize> = selected.iter().collect();
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
        let track_idx = self.current_track;
        // 2026-09 协作修复：连奏延长长度，须在修改后广播「删旧长度 + 加新长度」。
        // `track` 可变借用 self.document，故先收集到本地 Vec 再统一追加，规避借用冲突。
        // 协作同步关闭时跳过收集。
        let collab_sync = self.collab_sync_enabled;
        let mut sync_entries: Vec<CollabTransformSyncEntry> = Vec::new();

        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(track_idx))
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
                            if collab_sync {
                                sync_entries.push((
                                    false,
                                    note.start_tick as f32,
                                    note.key as u16,
                                    current_length,
                                    note.velocity,
                                    note.channel,
                                    track_idx,
                                ));
                                sync_entries.push((
                                    true,
                                    note.start_tick as f32,
                                    note.key as u16,
                                    (note.end_tick - note.start_tick) as f32,
                                    note.velocity,
                                    note.channel,
                                    track_idx,
                                ));
                            }
                            tied += 1;
                        }
                    }
                }
            }
        }
        self.pending_collab_transform_sync.extend(sync_entries);

        if tied > 0 {
            self.mark_current_track_changed();
        }
        tied
    }
}

/// 将降序索引列表合并为连续区间 `(index, count)`（降序，语义同 `RemoveAt`）。
///
/// `descending` 已由大到小排序（且去重）。相邻索引差 1 视为同一连续删除段，
/// 合并为单条 `(段首(最小索引), 段长)`。降序下发保证 GPU 段内左移时高索引
/// 先处理、低索引仍有效，与逐音符降序删除语义一致。
pub(super) fn merge_descending_ranges(descending: &[usize]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut i = 0;
    while i < descending.len() {
        let seg_start = descending[i]; // 段内最大索引（降序起点）
        let mut seg_end = seg_start;
        while i + 1 < descending.len() && descending[i + 1] == seg_end - 1 {
            seg_end -= 1;
            i += 1;
        }
        ranges.push((seg_end, seg_start - seg_end + 1));
        i += 1;
    }
    ranges
}

/// 从升序选中位图直接构建**降序**连续区间（免物化 O(K) 索引 Vec 与排序）
///
/// 升序遍历命中位，合并相邻索引为连续段，最后整体反转为降序输出
/// （与 [`merge_descending_ranges`] 输出格式一致，供 `remove_note_ranges` 使用）。
///
/// 成本：O(K + 段数)；整轨全选时仅 1 段、无 153MB 索引 Vec 中转。
pub(super) fn descending_ranges_from_selection(selected: &SelectionSet) -> Vec<(usize, usize)> {
    let mut ascending: Vec<(usize, usize)> = Vec::new();
    let mut run_start: Option<usize> = None;
    let mut prev = 0usize;
    for i in selected.iter() {
        match run_start {
            None => run_start = Some(i),
            Some(s) => {
                if i != prev + 1 {
                    ascending.push((s, prev - s + 1));
                    run_start = Some(i);
                }
            }
        }
        prev = i;
    }
    if let Some(s) = run_start {
        ascending.push((s, prev - s + 1));
    }
    ascending.reverse();
    ascending
}
