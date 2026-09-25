//! 音符选择与删除（自 `notes.rs` 拆分，保持各文件 < 400 行）

use super::super::super::selection_set::SelectionSet;

use super::EditorData;
use super::editing::merge_descending_ranges;

impl EditorData {
    /// 通过索引删除单个音符（直接操作 document 当前轨）
    pub fn delete_note_by_index(&mut self, index: usize) {
        if self.remove_note(self.current_track, index).is_some() {
            self.push_history();
            self.mark_current_track_changed();
        }
    }

    /// 批量删除选中音符
    ///
    /// 索引降序逐个删除 document 当前轨（唯一权威源），避免索引漂移；
    /// 删除完成后将待删索引合并为连续区间，统一下发 `RemoveAt { index, count }`
    /// 增量事件（按降序），替代原每音符一条 `RemoveAt { count: 1 }`：
    /// GPU 段内左移次数从 K（选中数）降至「连续删除段数」，整轨框选等海量选中
    /// 场景下避免 K 次整段搬移的全量级开销。
    /// 全程走 `note_delta_events` 增量通道，`note_delta_dirty` 不置位，
    /// 渲染层不触发全量兜底重建（不走全量重建）。
    pub fn delete_selected_notes(&mut self, selected: &SelectionSet) {
        if selected.is_empty() {
            return;
        }
        let indices: Vec<usize> = selected.iter().collect();
        let deleted = self.remove_notes_merged(self.current_track, &indices);
        if deleted == 0 {
            return;
        }
        self.push_history();
        self.mark_current_track_changed();
    }

    /// 批量删除指定音轨的若干索引，并按**区间归并**记录增量删除（不置全量脏标记）。
    ///
    /// 索引内部降序排序去重后逐个删除（避免索引漂移）；连续区间合并为单条
    /// `(index, count)` 事件，按轨分流：
    /// - 当前轨 → `note_delta_events`（主轨段内 `RemoveAt` 增量）
    /// - 其他轨 → `pending_track_remove_ranges`（UI 层转 `TrackRemoveRanges`
    ///   区间级增量，**替代整轨 `TrackDelta` 重建**）
    ///
    /// 返回实际删除数。调用方负责 `mark_track_notes_changed_for(Some(affected))`
    /// 与历史入栈（保持标记/历史语义由操作入口统一控制）。
    ///
    /// 同帧对同轨多次调用时，后续区间追加在原区间之后：各区间按记录顺序
    /// 依次应用（后一批索引基于前一批删除后的状态），语义正确。
    pub fn remove_notes_merged(&mut self, track_id: usize, indices: &[usize]) -> usize {
        if indices.is_empty() {
            return 0;
        }
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        sorted.dedup();

        let mut deleted = 0usize;
        if let Some(doc) = self.document.as_mut() {
            for &idx in &sorted {
                if doc.remove_note(track_id, idx).is_some() {
                    deleted += 1;
                }
            }
        }
        if deleted == 0 {
            return 0;
        }

        let ranges = merge_descending_ranges(&sorted);
        if track_id == self.current_track {
            for (index, count) in ranges {
                self.note_delta_events
                    .push(super::super::NoteDeltaEvent::RemoveAt { index, count });
            }
        } else if let Some(entry) = self
            .pending_track_remove_ranges
            .iter_mut()
            .find(|(t, _)| *t == track_id)
        {
            entry.1.extend(ranges);
        } else {
            self.pending_track_remove_ranges.push((track_id, ranges));
        }
        deleted
    }

    /// 取出并清空非当前轨待同步的区间删除（UI 层每帧消费 → `TrackRemoveRanges`）
    pub fn take_pending_track_remove_ranges(&mut self) -> Vec<(usize, Vec<(usize, usize)>)> {
        std::mem::take(&mut self.pending_track_remove_ranges)
    }

    /// 返回所有音符索引
    pub fn select_all_notes(&self) -> SelectionSet {
        (0..self.current_track_note_count()).collect()
    }

    /// 计算选择框内的音符索引（委托到 get_notes_in_selection_box，消除重复逻辑）
    pub fn compute_selection(
        &self,
        start_tick: f32,
        start_key: u16,
        current_tick: f32,
        current_key: u16,
    ) -> SelectionSet {
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
