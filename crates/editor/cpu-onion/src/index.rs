//! 派生索引：只存聚合信息，不复制任何音符数据。
//!
//! - `max_len[track]`：该轨最长音符的 tick 长度，用于 `ChunkedList::window_range`
//!   的精确 lookback。按轨取最大值而不是全曲最大值，避免为少数长音符把每一帧
//!   的扫描窗口放大到几十小节。
//! - `long_notes[track]`：超过 [`LOOKBACK_TICKS`] 的音符，渲染时作为长音符
//!   候选单独合并（按 `start_tick` 与正常窗口音符归并，保持源顺序）。
//! - `measure_counts[measure]`：每小节 onset 数，供诊断估算与密集段定位；
//!   编辑增量通过 [`TrackIndex::with_changes`] 维护。**不参与总览档位判定**
//!   （档位只看缩放，见 `density::uses_overview`）。
//! - `track_note_counts[track]`：每轨音符数，供 [`TrackIndex::rebuild_track_geometry`]
//!   在不重扫全文档的前提下精确修正 `total_notes`。
//! - `short_note_ticks`：音符长度第 10 百分位（至少 1 tick），供总览 LOD
//!   判断短音符在当前缩放下是否已不足 1px。
//!
//! `MidiDocument::clone` 是块级浅拷贝（音符块 `Arc` 共享），本索引同样只做
//! 聚合拷贝，不触碰音符数据。
//!
//! 增量维护方法（`with_changes*` / `rebuild_track_geometry`）见子模块
//! [`incremental`]。

use lumino_midi_model::{MidiDocument, NoteEvent};

mod incremental;

/// 正常窗口的 lookback 上限（tick）。
///
/// 99.95% 的音符 ≤ 512 tick；更长的音符单独放入 `long_notes`，逐帧只扫描
/// 这一小部分，避免为少数长音符在密集段把每个 band 的窗口放大到几百万条。
pub const LOOKBACK_TICKS: u32 = 512;

/// `measure_counts` 的保护上限，防止异常 tick 触发巨量内存分配。
///
/// 1,048,576 小节 ≈ 4/4、PPQ=1920 下约 134 亿 tick，远超正常工程规模；
/// 超出部分不参与诊断估算（音符渲染与长音符索引不受影响）。
const MAX_MEASURE_BUCKETS: usize = 1_048_576;

/// 短音符长度直方图上限（tick）；更长的音符并入末桶。
const SHORT_HISTOGRAM_TICKS: usize = 4096;

/// key 掩码单元上限（measures × tracks × 16B）；超过则不建，回退全画布覆盖目标。
const MAX_KEY_MASK_CELLS: usize = 1_000_000;

/// 每轨 / 每小节聚合索引。
#[derive(Debug, Clone)]
pub struct TrackIndex {
    /// 每轨最大音符长度（tick）。
    pub max_len: Vec<u32>,
    /// 音符长度第 10 百分位（至少 1 tick）；0 表示空文档。
    ///
    /// 总览 LOD 用它判断短音符是否**真正亚像素**（<1px）；1px 及以上仍画
    /// 原始音符，避免高缩放被误切热力图。
    pub short_note_ticks: u32,
    /// 超过 [`LOOKBACK_TICKS`] 的音符（按轨，按 `start_tick` 升序）。
    pub long_notes: Vec<Vec<NoteEvent>>,
    /// 长音符总数。
    pub long_note_count: u64,
    /// 每小节 onset 数（4/4 小节；仅用于诊断估算 / 基准定位）。
    pub measure_counts: Vec<u32>,
    /// 一个小节的 tick 数（PPQ × 4，至少为 1）。
    pub measure_ticks: u32,
    /// 密度最高的小节序号。
    pub densest_measure: usize,
    /// 密度最高小节的 onset 数。
    pub densest_measure_notes: u32,
    /// 总音符数。
    pub total_notes: u64,
    /// 每轨音符数（`rebuild_track_geometry` 用它把 `total_notes` 修正到精确值）。
    pub track_note_counts: Vec<u64>,
    /// 每轨、每小节的 key 位图（`key_masks[track][measure]`）。
    ///
    /// 用于 Exact 覆盖目标：只把本视口实际出现过的 key 行算进“应该铺满”的面积，
    /// 避免空 key 行让覆盖目标永远达不到。未建立时为空，调用方回退全画布。
    /// 删除音符时**只增不减**（陈旧位只会让目标偏大，不会少画像素）。
    pub(crate) key_masks: Vec<Vec<u128>>,
    /// key 掩码覆盖的小节数（0 = 未建立）。
    pub(crate) key_mask_measures: usize,
}

impl TrackIndex {
    /// 从文档构建聚合索引（O(总音符数)，只读遍历，不复制数据）。
    #[must_use]
    pub fn build(document: &MidiDocument) -> Self {
        let measure_ticks = u32::from(document.division()).saturating_mul(4).max(1);
        let track_count = document.notes.len();
        let mut max_len = vec![0u32; track_count];
        let mut long_notes: Vec<Vec<NoteEvent>> = (0..track_count).map(|_| Vec::new()).collect();
        let mut measure_counts = vec![0u32; 1];
        let mut track_note_counts = vec![0u64; track_count];
        let mut long_note_count = 0u64;
        let mut total_notes = 0u64;
        let mut length_histogram = vec![0u64; SHORT_HISTOGRAM_TICKS + 1];
        let estimated_measures =
            (document.total_ticks() as usize / measure_ticks as usize).saturating_add(2);
        let mut key_mask_measures = if estimated_measures
            .saturating_mul(track_count)
            .le(&MAX_KEY_MASK_CELLS)
        {
            estimated_measures
        } else {
            0
        };
        let mut key_masks = if key_mask_measures > 0 {
            vec![vec![0u128; key_mask_measures]; track_count]
        } else {
            Vec::new()
        };

        for (track_idx, notes) in document.notes.iter().enumerate() {
            let mut track_max = 0u32;
            let mut track_notes = 0u64;
            for note in notes.iter() {
                let len = note.length();
                track_max = track_max.max(len);
                length_histogram[(len as usize).min(SHORT_HISTOGRAM_TICKS)] += 1;
                if key_mask_measures > 0 && note.key < 128 {
                    let first = (note.start_tick / measure_ticks) as usize;
                    let last = ((note.end_tick / measure_ticks) as usize)
                        .min(key_mask_measures.saturating_sub(1));
                    if first >= key_mask_measures {
                        key_masks.clear();
                        key_mask_measures = 0;
                    } else {
                        let bit = 1u128 << (note.key as u32);
                        let slot = &mut key_masks[track_idx];
                        for cell in &mut slot[first..=last] {
                            *cell |= bit;
                        }
                    }
                }
                if len > LOOKBACK_TICKS
                    && let Some(slot) = long_notes.get_mut(track_idx)
                {
                    slot.push(*note);
                    long_note_count = long_note_count.saturating_add(1);
                }
                let measure = (note.start_tick / measure_ticks) as usize;
                grow_measure_buckets(&mut measure_counts, measure);
                if let Some(count) = measure_counts.get_mut(measure) {
                    *count = count.saturating_add(1);
                }
                track_notes = track_notes.saturating_add(1);
                total_notes = total_notes.saturating_add(1);
            }
            if let Some(slot) = max_len.get_mut(track_idx) {
                *slot = track_max;
            }
            if let Some(slot) = track_note_counts.get_mut(track_idx) {
                *slot = track_notes;
            }
        }

        let mut index = Self {
            max_len,
            short_note_ticks: short_percentile(&length_histogram, total_notes),
            long_notes,
            long_note_count,
            measure_counts,
            measure_ticks,
            densest_measure: 0,
            densest_measure_notes: 0,
            total_notes,
            track_note_counts,
            key_masks,
            key_mask_measures,
        };
        index.recompute_densest();
        index
    }

    /// 该轨渲染窗口的向左 lookback（tick）。
    #[inline]
    #[must_use]
    pub fn lookback(&self, track: usize) -> u32 {
        self.max_len
            .get(track)
            .map_or(0, |len| (*len).min(LOOKBACK_TICKS))
    }

    /// 该轨的长音符（长度 > [`LOOKBACK_TICKS`]），按 `start_tick` 升序。
    #[inline]
    #[must_use]
    pub fn long_notes(&self, track: usize) -> &[NoteEvent] {
        self.long_notes.get(track).map_or(&[], Vec::as_slice)
    }

    /// 视口 tick 范围内实际出现过的 key 位图；未建掩码时返回全 1（保守）。
    ///
    /// 当前轨与静音轨不参与：它们不由 CPU 洋葱皮绘制，不能算进覆盖目标。
    /// 查询向前扩 [`LOOKBACK_TICKS`]，覆盖从视口左侧跨入的普通音符。
    #[must_use]
    pub fn active_keys(
        &self,
        tick_lo: u32,
        tick_hi: u32,
        current_track: usize,
        muted: &[bool],
    ) -> u128 {
        if self.key_masks.is_empty() || self.key_mask_measures == 0 {
            return u128::MAX;
        }
        let measure_ticks = self.measure_ticks.max(1);
        let first = (tick_lo.saturating_sub(LOOKBACK_TICKS) / measure_ticks) as usize;
        let last =
            ((tick_hi / measure_ticks) as usize).min(self.key_mask_measures.saturating_sub(1));
        if first >= self.key_mask_measures || first > last {
            return 0;
        }
        let mut mask = 0u128;
        for (track, slots) in self.key_masks.iter().enumerate() {
            if track == current_track || muted.get(track).copied().unwrap_or(false) {
                continue;
            }
            for &slot in &slots[first..=last] {
                mask |= slot;
            }
        }
        mask
    }

    /// key 掩码占用字节数（诊断/内存预算用）。
    #[must_use]
    pub fn key_mask_bytes(&self) -> usize {
        self.key_masks
            .iter()
            .map(|slots| slots.len().saturating_mul(std::mem::size_of::<u128>()))
            .sum()
    }

    /// 把一个音符写入 key 掩码（插入/重建几何时用）。
    ///
    /// 掩码容纳不下时整体清空并回退全画布目标，绝不让目标偏小。
    pub(crate) fn mark_key_mask(&mut self, track: usize, note: &NoteEvent) {
        if self.key_mask_measures == 0 || note.key >= 128 {
            return;
        }
        let measure_ticks = self.measure_ticks.max(1);
        let first = (note.start_tick / measure_ticks) as usize;
        let last = ((note.end_tick / measure_ticks) as usize)
            .min(self.key_mask_measures.saturating_sub(1));
        if first >= self.key_mask_measures {
            self.key_masks.clear();
            self.key_mask_measures = 0;
            return;
        }
        if track >= self.key_masks.len() {
            let measures = self.key_mask_measures;
            if track.saturating_add(1).saturating_mul(measures) > MAX_KEY_MASK_CELLS {
                self.key_masks.clear();
                self.key_mask_measures = 0;
                return;
            }
            self.key_masks
                .resize_with(track + 1, || vec![0u128; measures]);
        }
        let bit = 1u128 << (note.key as u32);
        if let Some(slots) = self.key_masks.get_mut(track) {
            for cell in &mut slots[first..=last] {
                *cell |= bit;
            }
        }
    }

    /// 估算 `[tick_start, tick_end]` 内的音符数量（只读小节聚合计数）。
    ///
    /// 误差来自跨小节边界的音符；只用于诊断/基准估算，不参与总览档位判定。
    /// 超出 `measure_counts` 保护上限的 tick 返回已覆盖部分的估算值。
    #[must_use]
    pub fn estimate_visible(&self, tick_start: u32, tick_end: u32) -> u64 {
        if tick_end < tick_start || self.measure_counts.is_empty() {
            return 0;
        }
        let measure_ticks = self.measure_ticks.max(1);
        let first = (tick_start / measure_ticks) as usize;
        let last = (tick_end / measure_ticks) as usize;
        if first >= self.measure_counts.len() {
            return 0;
        }
        let last = last.min(self.measure_counts.len() - 1);
        if first > last {
            return 0;
        }
        self.measure_counts[first..=last]
            .iter()
            .map(|count| u64::from(*count))
            .sum()
    }

    /// 重算「密度最高的小节」（O(小节数)）。
    ///
    /// 只服务 LOD 估算与基准定位，**CPU 洋葱皮渲染不使用 densest**；编辑增量
    /// 路径 [`TrackIndex::with_changes_fast`] 刻意跳过它，避免每次单笔编辑都
    /// 扫一遍上百万吨的小节桶。
    pub(super) fn recompute_densest(&mut self) {
        let mut densest_measure = 0usize;
        let mut densest_notes = 0u32;
        for (measure, count) in self.measure_counts.iter().enumerate() {
            if *count > densest_notes {
                densest_notes = *count;
                densest_measure = measure;
            }
        }
        self.densest_measure = densest_measure;
        self.densest_measure_notes = densest_notes;
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

/// 按需扩容小节计数桶：几何增长，避免多轨递增 tick 时的重复拷贝；
/// 上限 [`MAX_MEASURE_BUCKETS`] 防止异常 tick 触发巨量分配。
fn grow_measure_buckets(counts: &mut Vec<u32>, measure: usize) {
    if measure >= MAX_MEASURE_BUCKETS || measure < counts.len() {
        return;
    }
    let needed = measure + 1;
    let grown = needed
        .max(counts.len().saturating_mul(2))
        .min(MAX_MEASURE_BUCKETS);
    counts.resize(grown, 0);
}

#[cfg(test)]
mod tests;
