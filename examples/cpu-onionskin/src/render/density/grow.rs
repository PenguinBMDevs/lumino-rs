//! 密度/覆盖布局扩容：编辑把音符写到既有右界之外时只追加空 bin。

use super::{
    DensityPyramid, KEY_COUNT,
    coverage::{EMPTY_CELL, MAX_COVERAGE_CELLS},
};

/// 保证 `[start, end)` 落在布局内；超内存上限时返回 `false`（调用方整建）。
pub(super) fn ensure_layout(pyramid: &mut DensityPyramid, start_tick: u32, end_tick: u32) -> bool {
    if pyramid.levels.is_empty() {
        return false;
    }
    let max_tick = end_tick.max(start_tick) as usize;
    let needed_base_bins = (max_tick / super::BASE_BIN_TICKS as usize).saturating_add(1);
    if needed_base_bins <= pyramid.levels[0].bins && end_tick <= pyramid.total_ticks {
        return true;
    }
    // 覆盖层基础 bin 可能比计数层更细（8 vs 64 tick），内存上限按更细的算。
    let coverage_bin = pyramid
        .coverage
        .first()
        .map_or(super::BASE_BIN_TICKS, |level| level.bin_ticks.max(1));
    let coverage_bins = (max_tick / coverage_bin as usize).saturating_add(1);
    if coverage_bins > MAX_COVERAGE_CELLS / KEY_COUNT {
        return false;
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
        let needed_base = needed_base_bins.max(coverage_bins).max(base.bins);
        for level in 0..pyramid.coverage.len() {
            let needed = if level == 0 {
                needed_base
            } else {
                needed_base.div_ceil(1usize << level)
            };
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
