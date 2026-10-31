//! 覆盖层脏区局部重建：只清空受影响 bin、只扫该 tick 区间的候选音符。

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::{
    DensityLevel, DensityPyramid, KEY_COUNT,
    coverage::{CoverageLevel, EMPTY_CELL, add_track, merge_cell},
};
use crate::model::TrackIndex;

pub(super) fn rebuild_dirty_coverage(
    pyramid: &mut DensityPyramid,
    document: &MidiDocument,
    index: &TrackIndex,
) -> bool {
    let Some((lo, hi)) = pyramid.coverage_dirty else {
        return false;
    };
    if pyramid.coverage.is_empty() {
        return false;
    }
    let bin_ticks = pyramid.coverage[0].bin_ticks.max(1);
    let bins = pyramid.coverage[0].bins.max(1);
    let first_bin = ((lo / bin_ticks) as usize).min(bins.saturating_sub(1));
    let last_bin = ((hi.saturating_sub(1) / bin_ticks) as usize).min(bins.saturating_sub(1));

    {
        let base = &mut pyramid.coverage[0];
        for bin in first_bin..=last_bin {
            let start = bin.saturating_mul(KEY_COUNT);
            let end = start.saturating_add(KEY_COUNT).min(base.cells.len());
            if let Some(cells) = base.cells.get_mut(start..end) {
                cells.fill(EMPTY_CELL);
            }
        }
        for track in 0..document.track_count() {
            let Ok(track_id) = u16::try_from(track) else {
                continue;
            };
            let notes = document.track_notes(track);
            let (window_lo, window_hi) =
                notes.window_range(lo, hi.saturating_add(1), index.lookback(track));
            for (_, note) in notes.iter_window(window_lo, window_hi) {
                add_note_cells(base, track_id, note, bin_ticks, first_bin, last_bin);
            }
            for note in index.long_notes(track) {
                if note.start_tick > hi {
                    break;
                }
                if note.end_tick <= lo {
                    continue;
                }
                add_note_cells(base, track_id, note, bin_ticks, first_bin, last_bin);
            }
        }
    }

    for level in 1..pyramid.coverage.len() {
        let first = first_bin >> level;
        let last = (last_bin >> level).min(pyramid.coverage[level].bins.saturating_sub(1));
        let (before, after) = pyramid.coverage.split_at_mut(level);
        let previous = &before[level - 1];
        let current = &mut after[0];
        for bin in first.min(last)..=last {
            for key in 0..KEY_COUNT {
                let cell = merge_cell(
                    previous
                        .cells
                        .get(bin * 2 * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(EMPTY_CELL),
                    previous
                        .cells
                        .get((bin * 2 + 1) * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(EMPTY_CELL),
                );
                if let Some(slot) = current
                    .cells
                    .get_mut(bin.saturating_mul(KEY_COUNT).saturating_add(key))
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

/// 按多个脏 tick 区间重建计数层 + 覆盖层（历史大变更）。
pub(super) fn rebuild_dirty_ranges(
    pyramid: &mut DensityPyramid,
    document: &MidiDocument,
    index: &TrackIndex,
    ranges: &[(u32, u32)],
) -> bool {
    let ranges = merge_ranges(ranges);
    if ranges.is_empty() || pyramid.levels.is_empty() {
        return false;
    }
    let start = ranges.first().map_or(0, |range| range.0);
    let end = ranges.last().map_or(start, |range| range.1);
    if !super::grow::ensure_layout(pyramid, start, end) {
        return false;
    }
    rebuild_counts(pyramid, document, &ranges);
    rebuild_coverage_ranges(pyramid, document, index, &ranges);
    true
}

fn merge_ranges(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = ranges.iter().copied().filter(|(lo, hi)| hi > lo).collect();
    ranges.sort_unstable_by_key(|range| range.0);
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (lo, hi) in ranges {
        match merged.last_mut() {
            Some(last) if lo <= last.1 => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

fn rebuild_counts(pyramid: &mut DensityPyramid, document: &MidiDocument, ranges: &[(u32, u32)]) {
    let base_bin = pyramid.levels[0].bin_ticks.max(1);
    {
        let base = &mut pyramid.levels[0];
        for &(lo, hi) in ranges {
            let first = (lo / base_bin) as usize;
            let last = (hi.saturating_sub(1) / base_bin) as usize;
            clear_count_rows(base, first, last);
        }
        for &(lo, hi) in ranges {
            let scan_lo = (lo / base_bin) * base_bin;
            let scan_hi = hi
                .div_ceil(base_bin)
                .saturating_mul(base_bin)
                .min(pyramid.total_ticks.max(scan_lo.saturating_add(1)));
            for track in 0..document.track_count() {
                let notes = document.track_notes(track);
                let (start, end) = notes.window_range(scan_lo, scan_hi, 0);
                for (_, note) in notes.iter_window(start, end) {
                    if note.start_tick < scan_lo || note.start_tick >= scan_hi {
                        continue;
                    }
                    let bin = (note.start_tick / base_bin) as usize;
                    if let Some(cell) = base.cells.get_mut(
                        bin.saturating_mul(KEY_COUNT)
                            .saturating_add(note.key as usize),
                    ) {
                        *cell = cell.saturating_add(1);
                    }
                }
            }
        }
    }

    for level in 1..pyramid.levels.len() {
        let (before, after) = pyramid.levels.split_at_mut(level);
        let previous = &before[level - 1];
        let current = &mut after[0];
        let bin_ticks = current.bin_ticks.max(1);
        for &(lo, hi) in ranges {
            let first = (lo / bin_ticks) as usize;
            let last =
                ((hi.saturating_sub(1) / bin_ticks) as usize).min(current.bins.saturating_sub(1));
            for bin in first.min(last)..=last {
                for key in 0..KEY_COUNT {
                    let a = previous
                        .cells
                        .get(bin * 2 * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(0);
                    let b = previous
                        .cells
                        .get((bin * 2 + 1) * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(0);
                    if let Some(cell) = current
                        .cells
                        .get_mut(bin.saturating_mul(KEY_COUNT).saturating_add(key))
                    {
                        *cell = a.saturating_add(b);
                    }
                }
            }
        }
    }

    for (index, level) in pyramid.levels.iter().enumerate() {
        if let Some(max) = pyramid.max_counts.get_mut(index) {
            *max = level.cells.iter().copied().max().unwrap_or(1).max(1);
        }
    }
}

fn clear_count_rows(level: &mut DensityLevel, first: usize, last: usize) {
    let last = last.min(level.bins.saturating_sub(1));
    for bin in first.min(last)..=last {
        let start = bin.saturating_mul(KEY_COUNT);
        let end = start.saturating_add(KEY_COUNT).min(level.cells.len());
        if let Some(cells) = level.cells.get_mut(start..end) {
            cells.fill(0);
        }
    }
}

fn rebuild_coverage_ranges(
    pyramid: &mut DensityPyramid,
    document: &MidiDocument,
    index: &TrackIndex,
    ranges: &[(u32, u32)],
) {
    let Some(base) = pyramid.coverage.first() else {
        return;
    };
    let bin_ticks = base.bin_ticks.max(1);
    {
        let base = &mut pyramid.coverage[0];
        for &(lo, hi) in ranges {
            let first = (lo / bin_ticks) as usize;
            let last = (hi.saturating_sub(1) / bin_ticks) as usize;
            clear_coverage_rows(base, first, last);
        }
        for &(lo, hi) in ranges {
            let first = (lo / bin_ticks) as usize;
            let last = (hi.saturating_sub(1) / bin_ticks) as usize;
            for track in 0..document.track_count() {
                let Ok(track_id) = u16::try_from(track) else {
                    continue;
                };
                let notes = document.track_notes(track);
                let (start, end) =
                    notes.window_range(lo, hi.saturating_add(1), index.lookback(track));
                for (_, note) in notes.iter_window(start, end) {
                    add_note_cells(base, track_id, note, bin_ticks, first, last);
                }
                for note in index.long_notes(track) {
                    if note.start_tick > hi {
                        break;
                    }
                    if note.end_tick <= lo {
                        continue;
                    }
                    add_note_cells(base, track_id, note, bin_ticks, first, last);
                }
            }
        }
    }

    for level in 1..pyramid.coverage.len() {
        let (before, after) = pyramid.coverage.split_at_mut(level);
        let previous = &before[level - 1];
        let current = &mut after[0];
        let bin_ticks = current.bin_ticks.max(1);
        for &(lo, hi) in ranges {
            let first = (lo / bin_ticks) as usize;
            let last =
                ((hi.saturating_sub(1) / bin_ticks) as usize).min(current.bins.saturating_sub(1));
            for bin in first.min(last)..=last {
                for key in 0..KEY_COUNT {
                    let cell = merge_cell(
                        previous
                            .cells
                            .get(bin * 2 * KEY_COUNT + key)
                            .copied()
                            .unwrap_or(EMPTY_CELL),
                        previous
                            .cells
                            .get((bin * 2 + 1) * KEY_COUNT + key)
                            .copied()
                            .unwrap_or(EMPTY_CELL),
                    );
                    if let Some(slot) = current
                        .cells
                        .get_mut(bin.saturating_mul(KEY_COUNT).saturating_add(key))
                    {
                        *slot = cell;
                    }
                }
            }
        }
    }
    pyramid.coverage_valid = true;
    pyramid.coverage_dirty = None;
}

fn clear_coverage_rows(level: &mut CoverageLevel, first: usize, last: usize) {
    let last = last.min(level.bins.saturating_sub(1));
    for bin in first.min(last)..=last {
        let start = bin.saturating_mul(KEY_COUNT);
        let end = start.saturating_add(KEY_COUNT).min(level.cells.len());
        if let Some(cells) = level.cells.get_mut(start..end) {
            cells.fill(EMPTY_CELL);
        }
    }
}

fn add_note_cells(
    base: &mut CoverageLevel,
    track: u16,
    note: &NoteEvent,
    bin_ticks: u32,
    first_bin: usize,
    last_bin: usize,
) {
    if note.end_tick <= note.start_tick || note.key as usize >= KEY_COUNT {
        return;
    }
    let first = ((note.start_tick / bin_ticks) as usize).max(first_bin);
    let last = ((note.end_tick.saturating_sub(1) / bin_ticks) as usize).min(last_bin);
    if first > last {
        return;
    }
    for bin in first..=last {
        let index = bin
            .saturating_mul(KEY_COUNT)
            .saturating_add(note.key as usize);
        if let Some(cell) = base.cells.get_mut(index) {
            add_track(cell, track);
        }
    }
}
