//! 彩色覆盖光栅：key × tick 单元记录**最先生效的轨道**（first-wins），
//! 直接按屏幕像素采样出图。
//!
//! 与蓝色密度热力图不同，这里保留真实轨道颜色与音符宽度：一个持续音覆盖
//! 多少个 tick 单元，就会在屏幕横向连续出多少像素。只有当一个屏幕像素内
//! 包含多个音符时，才按 GPU 洋葱皮的稳定序取第一条（轨道下标小者优先）。
//!
//! 每个单元同时记录第二条轨道：当前轨 / 静音轨恰好是第一条时，仍能取到
//! 下一条可见轨，而不是留下空洞。第三条及以后在单元粒度上不再区分——这个
//! 分辨率下同一像素已有两条候选，视觉差异小于 1px。
//!
//! 基础 bin = 8 tick，逐层 2:1 合并；内存上限 [`MAX_COVERAGE_CELLS`]。
//! 只在 `bin_ticks × zoom_x ≤ 2px` 时启用；更高缩放仍走 Exact 逐音符光栅。

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::DensityPyramid;
use crate::index::TrackIndex;

mod render;

/// 单层单元数上限（约 32 MiB 的 `u32`）。
pub(super) const MAX_COVERAGE_CELLS: usize = 1 << 25;
/// 基础 bin（tick）。
const BASE_BIN_TICKS: u32 = 8;
/// 层数：基础 bin × `2^0..=2^10`。
const LEVELS: usize = 11;
/// 空轨道槽。
const EMPTY_TRACK: u16 = u16::MAX;

/// 打包的单元：低 16 位第一条轨道，高 16 位第二条轨道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoverageCell(u32);

impl CoverageCell {
    pub(crate) const EMPTY: Self = Self(EMPTY_TRACK as u32 | ((EMPTY_TRACK as u32) << 16));

    #[inline]
    const fn first(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    #[inline]
    const fn second(self) -> u16 {
        (self.0 >> 16) as u16
    }

    #[inline]
    fn is_empty(self) -> bool {
        self.first() == EMPTY_TRACK
    }

    /// 加入一条轨道；已存在则不重复，第三条及以后丢弃。
    #[inline]
    pub(crate) fn add(&mut self, track: u16) {
        let first = self.first();
        if first == EMPTY_TRACK {
            self.0 = u32::from(track) | (u32::from(EMPTY_TRACK) << 16);
            return;
        }
        if first == track {
            return;
        }
        let second = self.second();
        if second == EMPTY_TRACK {
            self.0 = u32::from(first) | (u32::from(track) << 16);
        }
    }

    /// 合并两个单元：保留轨道下标最小的两条（必须按轨道序，不能按 bin 序）。
    #[inline]
    pub(crate) fn merge(self, other: Self) -> Self {
        let mut candidates = [EMPTY_TRACK; 4];
        let mut len = 0usize;
        for track in [self.first(), self.second(), other.first(), other.second()] {
            if track == EMPTY_TRACK || candidates[..len].contains(&track) {
                continue;
            }
            candidates[len] = track;
            len += 1;
        }
        candidates[..len].sort_unstable();
        let first = candidates[0];
        let second = if len > 1 { candidates[1] } else { EMPTY_TRACK };
        Self(u32::from(first) | (u32::from(second) << 16))
    }
}

/// 一层覆盖单元。
#[derive(Debug)]
pub(crate) struct CoverageLevel {
    /// 本层每个 bin 覆盖的 tick 数。
    pub(crate) bin_ticks: u32,
    /// bin 数。
    pub(crate) bins: usize,
    /// 行优先 `bin × key_count` 的单元。
    pub(crate) cells: Vec<CoverageCell>,
}

impl CoverageLevel {
    #[inline]
    pub(crate) fn get(&self, bin: usize, key: usize, keys: usize) -> CoverageCell {
        self.cells
            .get(bin.saturating_mul(keys).saturating_add(key))
            .copied()
            .unwrap_or(CoverageCell::EMPTY)
    }
}

/// 构建彩色覆盖金字塔（单线程按轨道顺序，保证 first-wins 稳定序）。
pub(crate) fn build_coverage(
    document: &MidiDocument,
    total_ticks: u32,
    key_count: u16,
) -> Vec<CoverageLevel> {
    let keys = key_count.max(1) as usize;
    let mut bin_ticks = BASE_BIN_TICKS;
    while (u64::from(total_ticks) / u64::from(bin_ticks))
        .saturating_add(2)
        .saturating_mul(keys as u64)
        > MAX_COVERAGE_CELLS as u64
    {
        bin_ticks = bin_ticks.saturating_mul(2);
        if bin_ticks == u32::MAX {
            break;
        }
    }
    let bins = ((u64::from(total_ticks) / u64::from(bin_ticks)) as usize).saturating_add(2);
    let mut cells = vec![CoverageCell::EMPTY; bins.saturating_mul(keys)];

    for track in 0..document.notes.len() {
        let track = u16::try_from(track).unwrap_or(EMPTY_TRACK);
        if track == EMPTY_TRACK {
            continue;
        }
        for note in document.track_notes(track as usize).iter() {
            if note.end_tick <= note.start_tick {
                continue;
            }
            let key = note.key as usize;
            if key >= keys {
                continue;
            }
            let first = (note.start_tick / bin_ticks) as usize;
            let last = (note.end_tick.saturating_sub(1) / bin_ticks) as usize;
            let last = last.min(bins.saturating_sub(1));
            if first >= bins || first > last {
                continue;
            }
            for bin in first..=last {
                let index = bin.saturating_mul(keys).saturating_add(key);
                if let Some(cell) = cells.get_mut(index) {
                    cell.add(track);
                }
            }
        }
    }

    build_levels_from_base(
        CoverageLevel {
            bin_ticks,
            bins,
            cells,
        },
        keys,
    )
}

/// 从基础层重建 2:1 金字塔（构建与脏区局部重建共用）。
fn build_levels_from_base(base: CoverageLevel, keys: usize) -> Vec<CoverageLevel> {
    let mut levels = Vec::with_capacity(LEVELS);
    levels.push(base);
    while levels.len() < LEVELS {
        let Some(previous) = levels.last() else {
            break;
        };
        if previous.bins <= 1 {
            break;
        }
        let next_bins = previous.bins.div_ceil(2);
        let mut next_cells = vec![CoverageCell::EMPTY; next_bins.saturating_mul(keys)];
        for bin in 0..next_bins {
            for key in 0..keys {
                let cell =
                    previous
                        .get(bin * 2, key, keys)
                        .merge(previous.get(bin * 2 + 1, key, keys));
                if let Some(slot) = next_cells.get_mut(bin.saturating_mul(keys).saturating_add(key))
                {
                    *slot = cell;
                }
            }
        }
        levels.push(CoverageLevel {
            bin_ticks: previous.bin_ticks.saturating_mul(2),
            bins: next_bins,
            cells: next_cells,
        });
    }
    levels
}

/// 只重建脏 tick 区间覆盖的单元：清空这些 bin，再按轨道序扫描该区间的
/// 音符（含从左侧跨入的普通音符与 `long_notes`）。
///
/// 返回 `false` 表示没有可用的脏区信息，调用方应退回整建。
pub(crate) fn rebuild_dirty_coverage(
    pyramid: &mut DensityPyramid,
    document: &MidiDocument,
    index: &TrackIndex,
) -> bool {
    let Some((lo, hi)) = pyramid.coverage_dirty else {
        return false;
    };
    let keys = pyramid.key_count as usize;
    if pyramid.coverage.is_empty() {
        return false;
    }
    let bin_ticks = pyramid.coverage[0].bin_ticks.max(1);
    let bins = pyramid.coverage[0].bins.max(1);
    let first_bin = ((lo / bin_ticks) as usize).min(bins.saturating_sub(1));
    let last_bin = ((hi.saturating_sub(1) / bin_ticks) as usize).min(bins.saturating_sub(1));

    // 只清空基础层脏 bin，然后扫描这些 tick 的候选音符。
    {
        let base = &mut pyramid.coverage[0];
        for bin in first_bin..=last_bin {
            let start = bin.saturating_mul(keys);
            let end = start.saturating_add(keys).min(base.cells.len());
            if let Some(cells) = base.cells.get_mut(start..end) {
                cells.fill(CoverageCell::EMPTY);
            }
        }
        for track in 0..document.notes.len() {
            let Ok(track_id) = u16::try_from(track) else {
                continue;
            };
            let notes = document.track_notes(track);
            let (window_lo, window_hi) =
                notes.window_range(lo, hi.saturating_add(1), index.lookback(track));
            for (_, note) in notes.iter_window(window_lo, window_hi) {
                add_note_cells(base, track_id, note, bin_ticks, keys, first_bin, last_bin);
            }
            for note in index.long_notes(track) {
                if note.start_tick > hi {
                    break;
                }
                if note.end_tick <= lo {
                    continue;
                }
                add_note_cells(base, track_id, note, bin_ticks, keys, first_bin, last_bin);
            }
        }
    }

    // 只重算受脏 bin 影响的上一层 cell，再逐层向上；不再重建整张基础层。
    for level in 1..pyramid.coverage.len() {
        let first = first_bin >> level;
        let last = last_bin >> level;
        let (before, after) = pyramid.coverage.split_at_mut(level);
        let previous = &before[level - 1];
        let current = &mut after[0];
        let last = last.min(current.bins.saturating_sub(1));
        for bin in first.min(last)..=last {
            for key in 0..keys {
                let cell =
                    previous
                        .get(bin * 2, key, keys)
                        .merge(previous.get(bin * 2 + 1, key, keys));
                if let Some(slot) = current
                    .cells
                    .get_mut(bin.saturating_mul(keys).saturating_add(key))
                {
                    *slot = cell;
                }
            }
        }
    }

    pyramid.coverage_valid = true;
    pyramid.coverage_dirty = None;
    true
}

fn add_note_cells(
    base: &mut CoverageLevel,
    track: u16,
    note: &NoteEvent,
    bin_ticks: u32,
    keys: usize,
    first_bin: usize,
    last_bin: usize,
) {
    if note.end_tick <= note.start_tick {
        return;
    }
    let key = note.key as usize;
    if key >= keys {
        return;
    }
    let first = ((note.start_tick / bin_ticks) as usize).max(first_bin);
    let last = ((note.end_tick.saturating_sub(1) / bin_ticks) as usize).min(last_bin);
    if first > last {
        return;
    }
    for bin in first..=last {
        let index = bin.saturating_mul(keys).saturating_add(key);
        if let Some(cell) = base.cells.get_mut(index) {
            cell.add(track);
        }
    }
}
