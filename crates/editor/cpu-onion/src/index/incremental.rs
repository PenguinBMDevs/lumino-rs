//! 索引的编辑增量维护（Step 5a）。
//!
//! 三个层次，成本严格递增：
//!
//! | 方法 | 成本 | 使用场景 |
//! |------|------|----------|
//! | [`TrackIndex::with_changes_fast`] | O(变更数 × 单轨长音符数) | 单笔插入/删除/更新（热路径） |
//! | [`TrackIndex::with_changes`] | 同上 + O(小节数) | 需要 `densest` 的工具/基准 |
//! | [`TrackIndex::rebuild_track_geometry`] | O(该轨音符数) | 当前轨被直接编辑后切轨补齐 |
//!
//! 都不重扫全文档、不复制任何音符数据。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::{LOOKBACK_TICKS, TrackIndex, grow_measure_buckets};

impl TrackIndex {
    /// 编辑后滚动更新派生索引：克隆一次，批量减旧增新。
    ///
    /// 只更新 `measure_counts` / `max_len` / `long_notes` / 总数；不重建整份
    /// 文档，也不复制音符数据。`max_len` 删除时允许偏大（只影响扫描窗口，安全）。
    ///
    /// 结尾会重算 `densest`（O(小节数)）——CPU 渲染路径请用
    /// [`Self::with_changes_fast`]。
    #[must_use]
    pub fn with_changes(&self, track: usize, removed: &[NoteEvent], added: &[NoteEvent]) -> Self {
        let mut next = self.apply_changes(track, removed, added);
        if track < self.max_len.len() {
            next.recompute_densest();
        }
        next
    }

    /// [`Self::with_changes`] 的**不重算 densest** 版本（CPU 洋葱皮热路径）。
    ///
    /// 语义与 `with_changes` 完全一致，唯一区别是刻意跳过 `densest_measure` /
    /// `densest_measure_notes` 的重算：这两个字段只服务诊断估算与基准定位，
    /// 渲染不使用；而每笔单笔编辑都扫一遍百万级小节桶会直接吃掉脏区重绘的收益。
    /// 代价是 full 兜底之前 densest 可能偏旧——不影响渲染正确性。
    #[must_use]
    pub fn with_changes_fast(
        &self,
        track: usize,
        removed: &[NoteEvent],
        added: &[NoteEvent],
    ) -> Self {
        self.apply_changes(track, removed, added)
    }

    /// 只重算某一轨的几何（`max_len` / `long_notes` / `long_note_count` / 音符数）。
    ///
    /// 用途：当前轨被**直接 `track_notes_mut` 编辑**（拖动 / 批量编辑 / 连奏）时
    /// 索引被刻意跳过更新；一旦切轨，该轨从「CPU 跳过」变成「CPU 要画」，必须
    /// 先把它的几何补齐，否则长音符候选与 lookback 窗口都是过期的。
    ///
    /// 成本 O(该轨音符数)。不重扫其它轨，**也不重算 `measure_of` 小节桶**：
    /// `measure_counts` 仅服务诊断估算，CPU 洋葱皮渲染不使用，保留偏保守的
    /// 旧值即可（准确度不影响像素输出）。`densest` 同理保持旧值。
    #[must_use]
    pub fn rebuild_track_geometry(&self, document: &MidiDocument, track: usize) -> Self {
        let mut next = self.clone();
        if track >= document.notes.len() {
            return next;
        }
        // 扩轨/删轨后向量长度可能与文档不一致：先补齐（越界视为空轨）
        if next.max_len.len() <= track {
            next.max_len.resize(track + 1, 0);
        }
        if next.long_notes.len() <= track {
            next.long_notes.resize_with(track + 1, Vec::new);
        }
        if next.track_note_counts.len() <= track {
            next.track_note_counts.resize(track + 1, 0);
        }

        let mut track_max = 0u32;
        let mut track_min = u32::MAX;
        let mut track_notes = 0u64;
        let mut longs: Vec<NoteEvent> = Vec::new();
        // `ChunkedList::iter` 按 start_tick 升序 → `longs` 天然满足长音符的升序不变式
        for note in document.track_notes(track).iter() {
            let len = note.length();
            track_max = track_max.max(len);
            track_min = track_min.min(len.max(1));
            track_notes = track_notes.saturating_add(1);
            if len > LOOKBACK_TICKS {
                longs.push(*note);
            }
            next.mark_key_mask(track, note);
        }
        if track_notes > 0 {
            // 直接编辑后短音符只可能变短；保留最小值仅作诊断统计。
            next.short_note_ticks = next.short_note_ticks.max(1).min(track_min);
        }

        let previous_notes = next.track_note_counts.get(track).copied().unwrap_or(0);
        let previous_longs = next.long_notes.get(track).map_or(0, |slot| slot.len()) as u64;
        let next_longs = longs.len() as u64;
        next.total_notes = next
            .total_notes
            .saturating_sub(previous_notes)
            .saturating_add(track_notes);
        next.long_note_count = next
            .long_note_count
            .saturating_sub(previous_longs)
            .saturating_add(next_longs);
        if let Some(slot) = next.max_len.get_mut(track) {
            *slot = track_max;
        }
        if let Some(slot) = next.long_notes.get_mut(track) {
            *slot = longs;
        }
        if let Some(slot) = next.track_note_counts.get_mut(track) {
            *slot = track_notes;
        }
        next
    }

    /// 批量减旧增新（不含 densest 维护）。
    fn apply_changes(&self, track: usize, removed: &[NoteEvent], added: &[NoteEvent]) -> Self {
        let mut next = self.clone();
        if track >= next.max_len.len() {
            return next;
        }
        for note in removed {
            next.apply_remove(track, *note);
        }
        for note in added {
            next.apply_insert(track, *note);
        }
        next
    }

    fn measure_of(&self, tick: u32) -> usize {
        (tick / self.measure_ticks.max(1)) as usize
    }

    fn apply_insert(&mut self, track: usize, note: NoteEvent) {
        self.total_notes = self.total_notes.saturating_add(1);
        self.mark_key_mask(track, &note);
        if let Some(count) = self.track_note_counts.get_mut(track) {
            *count = count.saturating_add(1);
        }
        let len = note.length();
        self.short_note_ticks = self.short_note_ticks.max(1).min(len.max(1));
        if let Some(slot) = self.max_len.get_mut(track) {
            *slot = (*slot).max(len);
        }
        let measure = self.measure_of(note.start_tick);
        grow_measure_buckets(&mut self.measure_counts, measure);
        if let Some(count) = self.measure_counts.get_mut(measure) {
            *count = count.saturating_add(1);
        }
        if len > LOOKBACK_TICKS
            && let Some(slot) = self.long_notes.get_mut(track)
        {
            let position = slot.partition_point(|item| item.start_tick <= note.start_tick);
            slot.insert(position, note);
            self.long_note_count = self.long_note_count.saturating_add(1);
        }
    }

    fn apply_remove(&mut self, track: usize, note: NoteEvent) {
        self.total_notes = self.total_notes.saturating_sub(1);
        if let Some(count) = self.track_note_counts.get_mut(track) {
            *count = count.saturating_sub(1);
        }
        let measure = self.measure_of(note.start_tick);
        if let Some(count) = self.measure_counts.get_mut(measure) {
            *count = count.saturating_sub(1);
        }
        if note.length() > LOOKBACK_TICKS
            && let Some(slot) = self.long_notes.get_mut(track)
        {
            let position = slot.partition_point(|item| item.start_tick < note.start_tick);
            let mut found = None;
            for (offset, item) in slot[position..]
                .iter()
                .enumerate()
                .take_while(|(_, item)| item.start_tick == note.start_tick)
            {
                if *item == note {
                    found = Some(position + offset);
                    break;
                }
            }
            if let Some(index) = found {
                slot.remove(index);
                self.long_note_count = self.long_note_count.saturating_sub(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MidiDocument {
        let mut document = MidiDocument::empty_with_tracks(2, 480);
        document.insert_note(0, NoteEvent::new(0, 100, 60, 100, 0));
        // 600 tick > LOOKBACK_TICKS，进入 long_notes。
        document.insert_note(0, NoteEvent::new(100, 700, 62, 100, 0));
        document.insert_note(1, NoteEvent::new(200, 300, 64, 100, 0));
        document
    }

    #[test]
    fn with_changes_updates_long_notes_and_counts() {
        let index = TrackIndex::build(&sample());
        let removed = [NoteEvent::new(100, 700, 62, 100, 0)];
        let added = [NoteEvent::new(0, 2000, 65, 100, 0)];
        let next = index.with_changes(0, &removed, &added);
        assert_eq!(next.total_notes, 3);
        assert_eq!(next.track_note_counts, vec![2, 1]);
        assert_eq!(next.max_len[0], 2000);
        assert_eq!(next.long_note_count, 1);
        assert!(next.long_notes(0).iter().any(|note| note.key == 65));
        assert!(!next.long_notes(0).iter().any(|note| note.key == 62));
        assert_eq!(next.estimate_visible(0, 1919), 3);
    }

    #[test]
    fn with_changes_fast_matches_build_geometry() {
        let index = TrackIndex::build(&sample());

        let mut edited = sample();
        edited.remove_note(0, 0);
        edited.insert_note(0, NoteEvent::new(0, 900, 67, 100, 0));
        edited.insert_note(0, NoteEvent::new(300, 1200, 60, 100, 0));

        let fast = index.with_changes_fast(
            0,
            &[NoteEvent::new(0, 100, 60, 100, 0)],
            &[
                NoteEvent::new(0, 900, 67, 100, 0),
                NoteEvent::new(300, 1200, 60, 100, 0),
            ],
        );
        let rebuilt = TrackIndex::build(&edited);
        assert_eq!(fast.max_len, rebuilt.max_len, "每轨最长音符必须等价");
        assert_eq!(fast.long_notes, rebuilt.long_notes, "长音符候选必须等价");
        assert_eq!(fast.long_note_count, rebuilt.long_note_count);
        assert_eq!(fast.total_notes, rebuilt.total_notes);
        assert_eq!(fast.track_note_counts, rebuilt.track_note_counts);
        assert_eq!(fast.measure_counts, rebuilt.measure_counts);
        // densest 被刻意跳过：不与 build 对齐（仅供 LOD/工具，渲染不使用）
    }

    #[test]
    fn with_changes_fast_ignores_out_of_range_track() {
        let index = TrackIndex::build(&sample());
        let next = index.with_changes_fast(99, &[], &[NoteEvent::new(0, 10, 60, 100, 0)]);
        assert_eq!(next.total_notes, index.total_notes);
    }

    #[test]
    fn rebuild_track_geometry_repairs_direct_edits() {
        let index = TrackIndex::build(&sample());

        // 模拟「当前轨被直接 track_notes_mut 编辑」后的文档
        let mut edited = sample();
        edited.remove_note(0, 1);
        edited.insert_note(0, NoteEvent::new(150, 5000, 70, 100, 0));

        let repaired = index.rebuild_track_geometry(&edited, 0);
        let rebuilt = TrackIndex::build(&edited);
        assert_eq!(repaired.max_len, rebuilt.max_len);
        assert_eq!(repaired.long_notes, rebuilt.long_notes);
        assert_eq!(repaired.long_note_count, rebuilt.long_note_count);
        assert_eq!(repaired.total_notes, rebuilt.total_notes);
        assert_eq!(repaired.track_note_counts, rebuilt.track_note_counts);
        // 其它轨完全不动
        assert_eq!(repaired.long_notes(1), index.long_notes(1));
        assert_eq!(repaired.max_len[1], index.max_len[1]);
    }

    #[test]
    fn rebuild_track_geometry_is_safe_for_out_of_range_track() {
        let index = TrackIndex::build(&sample());
        let next = index.rebuild_track_geometry(&sample(), 99);
        assert_eq!(next.max_len, index.max_len);
        assert_eq!(next.total_notes, index.total_notes);
    }

    #[test]
    fn rebuild_track_geometry_handles_empty_track() {
        let mut document = sample();
        document.replace_track_notes(0, Vec::new());
        let repaired = TrackIndex::build(&sample()).rebuild_track_geometry(&document, 0);
        assert_eq!(repaired.max_len[0], 0);
        assert!(repaired.long_notes(0).is_empty());
        assert_eq!(repaired.track_note_counts[0], 0);
        assert_eq!(repaired.total_notes, 1, "轨 1 的音符必须仍在统计内");
    }

    #[test]
    fn rebuild_track_geometry_grows_vectors_after_track_add() {
        let mut document = sample();
        document.add_empty_track();
        document.insert_note(2, NoteEvent::new(0, 1000, 72, 100, 0));
        let repaired = TrackIndex::build(&sample()).rebuild_track_geometry(&document, 2);
        assert_eq!(repaired.max_len.len(), 3);
        assert_eq!(repaired.max_len[2], 1000);
        assert_eq!(repaired.long_notes(2).len(), 1);
        assert_eq!(repaired.total_notes, 4);
    }
}
