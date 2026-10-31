//! 密度/覆盖布局扩容：编辑把音符写到既有右界之外时，只追加空 bin，
//! 保留既有计数与覆盖单元，不再整份重扫文档。
//!
//! 内存上限内直接追加空 bin；超过上限时把基础层 2:1 降采样（丢弃最细层、
//! 补一个更粗的顶层），再继续扩容——不重扫文档、不整建。

use super::coverage::{CoverageCell, MAX_COVERAGE_CELLS};
use super::{DensityLevel, DensityPyramid, LEVELS as COVERAGE_LEVELS, MAX_BASE_BINS};

/// 保证 `[start, end)` 落在布局内；必要时降采样 + 追加空 bin。
///
/// 返回 `false` 表示已经无法再降采样（极端总 tick），调用方必须整建。
pub(super) fn ensure_layout(pyramid: &mut DensityPyramid, start_tick: u32, end_tick: u32) -> bool {
    let keys = pyramid.key_count.max(1) as usize;
    loop {
        let Some(first) = pyramid.levels.first() else {
            return false;
        };
        let needed_counts =
            (end_tick.max(start_tick) as usize / first.bin_ticks.max(1) as usize).saturating_add(1);
        let coverage_bin = pyramid
            .coverage
            .first()
            .map_or(first.bin_ticks, |level| level.bin_ticks.max(1));
        let needed_coverage =
            (end_tick.max(start_tick) as usize / coverage_bin as usize).saturating_add(1);
        let over_counts = needed_counts > MAX_BASE_BINS;
        let over_coverage = needed_coverage > MAX_COVERAGE_CELLS / keys.max(1);
        if !over_counts && !over_coverage {
            break;
        }
        if !coarsen(pyramid) {
            return false;
        }
    }

    // 计数层：行优先，追加空行即可保持既有 2:1 合并关系。
    for level in &mut pyramid.levels {
        let needed = (end_tick as usize / level.bin_ticks.max(1) as usize).saturating_add(1);
        if needed > level.bins {
            level.cells.resize(needed.saturating_mul(keys), 0);
            level.bins = needed;
        }
    }

    grow_coverage(pyramid, end_tick, keys);
    pyramid.total_ticks = pyramid.total_ticks.max(end_tick);
    true
}

/// 丢弃最细层，把上一层当新基础层，并补一个更粗的顶层。
fn coarsen(pyramid: &mut DensityPyramid) -> bool {
    if pyramid.levels.len() < 2 || pyramid.coverage.len() < 2 {
        return false;
    }
    pyramid.levels.remove(0);
    let Some(last) = pyramid.levels.last() else {
        return false;
    };
    let bins = last.bins.div_ceil(2).max(1);
    let bin_ticks = last.bin_ticks.saturating_mul(2);
    let keys = pyramid.key_count.max(1) as usize;
    let mut cells = vec![0u16; bins.saturating_mul(keys)];
    for bin in 0..bins {
        for key in 0..keys {
            let a = last.cells.get(bin * 2 * keys + key).copied().unwrap_or(0);
            let b = last
                .cells
                .get((bin * 2 + 1) * keys + key)
                .copied()
                .unwrap_or(0);
            if let Some(cell) = cells.get_mut(bin.saturating_mul(keys).saturating_add(key)) {
                *cell = a.saturating_add(b);
            }
        }
    }
    pyramid.levels.push(DensityLevel {
        bin_ticks,
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
    let bin_ticks = last.bin_ticks.saturating_mul(2);
    let mut cells = vec![CoverageCell::EMPTY; bins.saturating_mul(keys)];
    for bin in 0..bins {
        for key in 0..keys {
            let cell = last
                .get(bin * 2, key, keys)
                .merge(last.get(bin * 2 + 1, key, keys));
            if let Some(slot) = cells.get_mut(bin.saturating_mul(keys).saturating_add(key)) {
                *slot = cell;
            }
        }
    }
    pyramid.coverage.push(super::coverage::CoverageLevel {
        bin_ticks,
        bins,
        cells,
    });
    true
}

/// 覆盖层追加空 bin；上层 bin 数按 2:1 同步增长，局部重建会补齐新单元。
fn grow_coverage(pyramid: &mut DensityPyramid, end_tick: u32, keys: usize) {
    let Some(base) = pyramid.coverage.first() else {
        return;
    };
    let coverage_bins = (end_tick as usize / base.bin_ticks.max(1) as usize).saturating_add(1);
    let needed_base_bins = coverage_bins.max(base.bins);
    for level in 0..pyramid.coverage.len().min(COVERAGE_LEVELS) {
        let needed = if level == 0 {
            needed_base_bins
        } else {
            needed_base_bins.div_ceil(1usize << level)
        };
        let current = &mut pyramid.coverage[level];
        if needed > current.bins {
            current
                .cells
                .resize(needed.saturating_mul(keys), CoverageCell::EMPTY);
            current.bins = needed;
        }
    }
}
