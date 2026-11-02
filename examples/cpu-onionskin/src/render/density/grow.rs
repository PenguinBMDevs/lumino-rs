//! 密度/覆盖布局扩容：内存上限内追加空 bin，超限时 2:1 降采样后继续。

use super::{
    DensityLevel, DensityPyramid, KEY_COUNT,
    coverage::{CoverageLevel, EMPTY_CELL, MAX_COVERAGE_CELLS, merge_cell},
};

/// 保证 `[start, end)` 落在布局内；必要时降采样，仍不整建。
pub(super) fn ensure_layout(pyramid: &mut DensityPyramid, start_tick: u32, end_tick: u32) -> bool {
    let max_tick = end_tick.max(start_tick) as usize;
    loop {
        let Some(first) = pyramid.levels.first() else {
            return false;
        };
        let coverage_bin = pyramid
            .coverage
            .first()
            .map_or(first.bin_ticks.max(1), |level| level.bin_ticks.max(1));
        let coverage_bins = (max_tick / coverage_bin as usize).saturating_add(1);
        if coverage_bins <= MAX_COVERAGE_CELLS / KEY_COUNT {
            break;
        }
        if !coarsen(pyramid) {
            return false;
        }
    }
    if end_tick <= pyramid.total_ticks
        && (max_tick / pyramid.levels[0].bin_ticks.max(1) as usize).saturating_add(1)
            <= pyramid.levels[0].bins
    {
        return true;
    }

    let mut needs = Vec::with_capacity(pyramid.levels.len());
    for level in &pyramid.levels {
        needs.push((max_tick / level.bin_ticks.max(1) as usize).saturating_add(1));
    }
    for (level, needed) in pyramid.levels.iter_mut().zip(needs) {
        if needed > level.bins {
            level.cells.resize(needed.saturating_mul(KEY_COUNT), 0);
            level.bins = needed;
        }
    }

    if let Some(base) = pyramid.coverage.first() {
        let coverage_bins = (max_tick / base.bin_ticks.max(1) as usize).saturating_add(1);
        let needed_base = coverage_bins.max(base.bins);
        for level in 0..pyramid.coverage.len() {
            let needed = needed_base.div_ceil(1usize << level);
            let current = &mut pyramid.coverage[level];
            if needed > current.bins {
                current
                    .cells
                    .resize(needed.saturating_mul(KEY_COUNT), EMPTY_CELL);
                current.bins = needed;
            }
        }
    }

    pyramid.total_ticks = pyramid.total_ticks.max(end_tick);
    true
}

fn coarsen(pyramid: &mut DensityPyramid) -> bool {
    if pyramid.levels.len() < 2 || pyramid.coverage.len() < 2 {
        return false;
    }
    pyramid.levels.remove(0);
    let Some(last) = pyramid.levels.last() else {
        return false;
    };
    let bins = last.bins.div_ceil(2).max(1);
    let mut cells = vec![0u16; bins.saturating_mul(KEY_COUNT)];
    for bin in 0..bins {
        for key in 0..KEY_COUNT {
            let a = last
                .cells
                .get(bin * 2 * KEY_COUNT + key)
                .copied()
                .unwrap_or(0);
            let b = last
                .cells
                .get((bin * 2 + 1) * KEY_COUNT + key)
                .copied()
                .unwrap_or(0);
            if let Some(cell) = cells.get_mut(bin.saturating_mul(KEY_COUNT).saturating_add(key)) {
                *cell = a.saturating_add(b);
            }
        }
    }
    pyramid.levels.push(DensityLevel {
        bin_ticks: last.bin_ticks.saturating_mul(2),
        bins,
        cells,
    });
    for (index, level) in pyramid.levels.iter().enumerate() {
        if let Some(max) = pyramid.max_counts.get_mut(index) {
            *max = level.cells.iter().copied().max().unwrap_or(1).max(1);
        }
    }

    pyramid.coverage.remove(0);
    let Some(last) = pyramid.coverage.last() else {
        return false;
    };
    let bins = last.bins.div_ceil(2).max(1);
    let mut cells = vec![EMPTY_CELL; bins.saturating_mul(KEY_COUNT)];
    for bin in 0..bins {
        for key in 0..KEY_COUNT {
            let cell = merge_cell(
                last.cells
                    .get(bin * 2 * KEY_COUNT + key)
                    .copied()
                    .unwrap_or(EMPTY_CELL),
                last.cells
                    .get((bin * 2 + 1) * KEY_COUNT + key)
                    .copied()
                    .unwrap_or(EMPTY_CELL),
            );
            if let Some(slot) = cells.get_mut(bin.saturating_mul(KEY_COUNT).saturating_add(key)) {
                *slot = cell;
            }
        }
    }
    pyramid.coverage.push(CoverageLevel {
        bin_ticks: last.bin_ticks.saturating_mul(2),
        bins,
        cells,
    });
    true
}
