//! 覆盖层脏区局部重建：只清空受影响 bin、只扫该 tick 区间的候选音符。

use lumino_midi_loader::{MidiDocument, NoteEvent};

use super::{
    DensityPyramid, KEY_COUNT,
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
