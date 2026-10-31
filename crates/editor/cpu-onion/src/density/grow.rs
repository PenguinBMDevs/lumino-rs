//! 密度/覆盖布局扩容：编辑把音符写到既有右界之外时，只追加空 bin，
//! 保留既有计数与覆盖单元，不再整份重扫文档。
//!
//! 只在基础 bin 宽度不变的前提下扩容——rebin 需要重算所有层，退化为整建；
//! 超过覆盖层安全内存上限时拒绝扩容，由调用方走整建选更粗的 bin。

use super::coverage::{CoverageCell, MAX_COVERAGE_CELLS};
use super::{DensityPyramid, LEVELS as COVERAGE_LEVELS};

/// 保证 `[start, end)` 落在既有布局内；必要时追加空 bin。
///
/// 返回 `false` 表示扩容会超过覆盖层内存上限，调用方必须整建。
pub(super) fn ensure_layout(pyramid: &mut DensityPyramid, start_tick: u32, end_tick: u32) -> bool {
    let Some(first) = pyramid.levels.first() else {
        return false;
    };
    let keys = pyramid.key_count.max(1) as usize;
    let needed_base_bins =
        (end_tick.max(start_tick) as usize / first.bin_ticks.max(1) as usize).saturating_add(1);
    if needed_base_bins <= first.bins && end_tick <= pyramid.total_ticks {
        return true;
    }
    // 覆盖层基础 bin 可能比计数层更细（8 vs 64 tick），内存上限按更细的算。
    let coverage_bin = pyramid
        .coverage
        .first()
        .map_or(first.bin_ticks, |level| level.bin_ticks.max(1));
    let coverage_bins = (end_tick as usize / coverage_bin as usize).saturating_add(1);
    if coverage_bins > MAX_COVERAGE_CELLS / keys.max(1) {
        return false;
    }

    // 计数层：行优先，追加空行即可保持既有 2:1 合并关系。
    for level in &mut pyramid.levels {
        let needed = (end_tick as usize / level.bin_ticks.max(1) as usize).saturating_add(1);
        if needed > level.bins {
            level.cells.resize(needed.saturating_mul(keys), 0);
            level.bins = needed;
        }
    }

    grow_coverage(pyramid, end_tick, needed_base_bins, keys);
    pyramid.total_ticks = pyramid.total_ticks.max(end_tick);
    true
}

/// 覆盖层追加空 bin；上层 bin 数按 2:1 同步增长，局部重建会补齐新单元。
fn grow_coverage(
    pyramid: &mut DensityPyramid,
    end_tick: u32,
    needed_base_bins: usize,
    keys: usize,
) {
    let Some(base) = pyramid.coverage.first() else {
        return;
    };
    let coverage_bins = (end_tick as usize / base.bin_ticks.max(1) as usize).saturating_add(1);
    let needed_base_bins = needed_base_bins.max(coverage_bins).max(base.bins);
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
