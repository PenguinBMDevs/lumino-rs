//! 派生索引：只存聚合信息，不复制任何音符数据。
//!
//! - `max_len[track]`：该轨最长音符的 tick 长度，用于 `ChunkedList::window_range`
//!   的精确 lookback。黑乐谱 75% 音符只有 1~7 tick，按轨取最大值而不是全曲
//!   最大值，可避免为少数长音符把每一帧的扫描窗口放大到几十小节。
//! - `measure_counts[measure]`：每小节 onset 数，用于基准密集段定位与诊断估算；
//!   **不参与总览档位判定**（档位只看同屏小节数，见 `render::density::uses_overview`）。
//! - `short_note_ticks`：音符长度的第 10 百分位（至少 1 tick），供列主序窄音符
//!   阈值使用；不参与总览 LOD。

use lumino_midi_loader::{MidiDocument, NoteEvent};

/// 短音符长度直方图的上限（tick）；更长的音符并入末桶。
const SHORT_HISTOGRAM_TICKS: usize = 4096;
/// key 掩码单元数上限（measures × tracks × 16B）；超过则不建，退回全画布覆盖目标。
const MAX_KEY_MASK_CELLS: usize = 1_000_000;

/// 正常窗口的 lookback 上限（tick）。
///
/// 99.95% 的音符 ≤ 512 tick（实测该 MIDI：>512 tick 仅 13.7 万 / 1.64 亿）。
/// 更长的音符单独放入 `long_notes`，逐帧只扫描这 13.7 万条，避免为少数长音符
/// 在密集段把每个 band 的窗口放大到几百万条。
pub const LOOKBACK_TICKS: u32 = 512;

/// 每轨/每小节聚合索引。
#[derive(Debug, Clone)]
pub struct TrackIndex {
    /// 每轨最大音符长度（tick）。
    pub max_len: Vec<u32>,
    /// 音符长度第 10 百分位（至少 1 tick）；0 表示空文档。
    pub short_note_ticks: u32,
    /// 超过 [`LOOKBACK_TICKS`] 的音符（按轨），渲染时作为长音符 underlay。
    pub long_notes: Vec<Vec<NoteEvent>>,
    /// 长音符总数。
    pub long_note_count: u64,
    /// 每小节 onset 数（4/4 小节；仅用于诊断估算/基准定位，不参与 LOD 判定）。
    pub measure_counts: Vec<u32>,
    /// 一个小节的 tick 数（PPQ × 4）。
    pub measure_ticks: u32,
    /// 密度最高的小节序号。
    pub densest_measure: usize,
    /// 密度最高小节的 onset 数。
    pub densest_measure_notes: u32,
    /// 总音符数。
    pub total_notes: u64,
    /// 每个（小节 × 音轨）的出现 key 位图，用于精确估计"本视口实际有哪些
    /// key 行需要绘制"，让覆盖提前退出不因空 key 行永远无法达标。
    pub key_masks: Vec<u128>,
    /// key 掩码的音轨数 / 小节数；掩码未建立时为 0。
    pub(crate) key_mask_tracks: usize,
    pub(crate) key_mask_measures: usize,
}

impl TrackIndex {
    /// 从文档构建聚合索引（O(总音符数)，只读遍历，不复制数据）。
    #[must_use]
    pub fn build(document: &MidiDocument) -> Self {
        let measure_ticks = (document.division() as u32).saturating_mul(4).max(1);
        let measures = document.total_ticks().div_ceil(measure_ticks) as usize + 1;
        let mut max_len = vec![0u32; document.track_count()];
        let mut long_notes = vec![Vec::new(); document.track_count()];
        let mut long_note_count = 0u64;
        let mut measure_counts = vec![0u32; measures];
        let mut total_notes = 0u64;
        let mut length_histogram = vec![0u64; SHORT_HISTOGRAM_TICKS + 1];
        let track_count = document.track_count();
        let key_mask_cells = measures.saturating_mul(track_count);
        let build_key_masks = key_mask_cells <= MAX_KEY_MASK_CELLS;
        let mut key_masks = if build_key_masks {
            vec![0u128; key_mask_cells]
        } else {
            Vec::new()
        };

        for (track_idx, notes) in document.notes.iter().enumerate() {
            let mut track_max = 0u32;
            for note in notes.iter() {
                let len = note.length();
                track_max = track_max.max(len);
                length_histogram[(len as usize).min(SHORT_HISTOGRAM_TICKS)] += 1;
                if build_key_masks {
                    let first = (note.start_tick / measure_ticks) as usize;
                    let last = ((note.end_tick / measure_ticks) as usize).min(measures - 1);
                    if first < measures {
                        let bit = 1u128 << (note.key & 127);
                        // 跨小节长音符也写入其覆盖的所有小节，避免视口只取
                        // 起点所在小节时漏掉仍在响的 key。
                        for measure in first..=last {
                            key_masks[measure * track_count + track_idx] |= bit;
                        }
                    }
                }
                if len > LOOKBACK_TICKS
                    && let Some(slot) = long_notes.get_mut(track_idx)
                {
                    slot.push(*note);
                    long_note_count += 1;
                }
                let measure = (note.start_tick / measure_ticks) as usize;
                if let Some(count) = measure_counts.get_mut(measure) {
                    *count = count.saturating_add(1);
                }
                total_notes += 1;
            }
            if let Some(slot) = max_len.get_mut(track_idx) {
                *slot = track_max;
            }
        }

        let short_note_ticks = short_percentile(&length_histogram, total_notes);

        let (densest_measure, densest_measure_notes) = measure_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| **count)
            .map_or((0, 0), |(index, count)| (index, *count));

        Self {
            max_len,
            short_note_ticks,
            long_notes,
            long_note_count,
            measure_counts,
            measure_ticks,
            densest_measure,
            densest_measure_notes,
            total_notes,
            key_masks,
            key_mask_tracks: if build_key_masks { track_count } else { 0 },
            key_mask_measures: if build_key_masks { measures } else { 0 },
        }
    }

    /// 该轨渲染窗口的向左 lookback（tick）。
    #[inline]
    #[must_use]
    pub fn lookback(&self, track: usize) -> u32 {
        self.max_len
            .get(track)
            .copied()
            .unwrap_or(0)
            .min(LOOKBACK_TICKS)
    }

    /// 该轨的长音符（长度 > [`LOOKBACK_TICKS`]）。
    #[inline]
    #[must_use]
    pub fn long_notes(&self, track: usize) -> &[NoteEvent] {
        self.long_notes.get(track).map_or(&[], Vec::as_slice)
    }

    /// 视口 tick 范围内实际出现过的 key 位图；未建掩码时返回全 1（保守）。
    ///
    /// `exclude_track` 用于洋葱皮覆盖目标：当前轨由主轨 pass 单独绘制，
    /// 不能算进"洋葱皮必须铺满"的面积里，否则目标永远达不到。
    #[must_use]
    pub fn active_keys(&self, tick_lo: u32, tick_hi: u32, exclude_track: Option<usize>) -> u128 {
        if self.key_mask_tracks == 0 || self.key_mask_measures == 0 {
            return u128::MAX;
        }
        let first = (tick_lo / self.measure_ticks.max(1)) as usize;
        let last = ((tick_hi / self.measure_ticks.max(1)) as usize)
            .min(self.key_mask_measures.saturating_sub(1));
        if first >= self.key_mask_measures || first > last {
            return 0;
        }
        let mut mask = 0u128;
        for measure in first..=last {
            let base = measure * self.key_mask_tracks;
            for track in 0..self.key_mask_tracks {
                if exclude_track == Some(track) {
                    continue;
                }
                mask |= self.key_masks[base + track];
            }
        }
        mask
    }

    /// 估计 `[tick_start, tick_end]` 内的音符数量（仅诊断/基准显示）。
    ///
    /// 只按小节聚合计数估算，不做逐音符扫描；误差来自跨小节边界的音符。
    /// **不参与总览档位判定**：档位与主程序一致，只看缩放物理像素。
    #[must_use]
    pub fn estimate_visible(&self, tick_start: u32, tick_end: u32) -> u64 {
        if tick_end < tick_start {
            return 0;
        }
        let first = (tick_start / self.measure_ticks) as usize;
        let last = (tick_end / self.measure_ticks) as usize;
        let mut total = 0u64;
        for measure in first..=last {
            if let Some(count) = self.measure_counts.get(measure) {
                total += u64::from(*count);
            }
        }
        total
    }

    /// 编辑后滚动更新派生索引：克隆一次，批量减旧增新。
    ///
    /// 只更新 `measure_counts` / `max_len` / `long_notes` / 总数；不重建整份
    /// 文档，也不复制音符数据。`max_len` 删除时允许偏大（只影响扫描窗口，安全）。
    #[must_use]
    pub fn with_changes(&self, track: usize, removed: &[NoteEvent], added: &[NoteEvent]) -> Self {
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

    fn apply_insert(&mut self, track: usize, note: NoteEvent) {
        self.total_notes = self.total_notes.saturating_add(1);
        self.short_note_ticks = self.short_note_ticks.max(1).min(note.length().max(1));
        self.max_len[track] = self.max_len[track].max(note.length());
        if let Some(count) = self
            .measure_counts
            .get_mut((note.start_tick / self.measure_ticks) as usize)
        {
            *count = count.saturating_add(1);
        }
        if note.length() > LOOKBACK_TICKS
            && let Some(slot) = self.long_notes.get_mut(track)
        {
            let position = slot.partition_point(|item| item.start_tick <= note.start_tick);
            slot.insert(position, note);
            self.long_note_count = self.long_note_count.saturating_add(1);
        }
    }

    fn apply_remove(&mut self, track: usize, note: NoteEvent) {
        self.total_notes = self.total_notes.saturating_sub(1);
        if let Some(count) = self
            .measure_counts
            .get_mut((note.start_tick / self.measure_ticks) as usize)
        {
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

/// 从长度直方图取第 10 百分位（至少 1 tick）。
fn short_percentile(histogram: &[u64], total_notes: u64) -> u32 {
    if total_notes == 0 {
        return 0;
    }
    let target = total_notes.saturating_mul(10).div_ceil(100).max(1);
    let mut accumulated = 0u64;
    for (ticks, count) in histogram.iter().enumerate() {
        accumulated = accumulated.saturating_add(*count);
        if accumulated >= target {
            return (ticks as u32).max(1);
        }
    }
    SHORT_HISTOGRAM_TICKS as u32
}

#[cfg(test)]
mod tests {
    use super::TrackIndex;

    #[test]
    fn estimate_visible_uses_measure_buckets() {
        let index = TrackIndex {
            max_len: vec![0],
            short_note_ticks: 1,
            long_notes: vec![Vec::new()],
            long_note_count: 0,
            measure_counts: vec![10, 20, 30],
            measure_ticks: 100,
            densest_measure: 2,
            densest_measure_notes: 30,
            total_notes: 60,
            key_masks: Vec::new(),
            key_mask_tracks: 0,
            key_mask_measures: 0,
        };
        assert_eq!(index.estimate_visible(0, 99), 10);
        assert_eq!(index.estimate_visible(0, 100), 30);
        assert_eq!(index.estimate_visible(150, 250), 50);
        assert_eq!(index.estimate_visible(300, 200), 0);
    }
}
