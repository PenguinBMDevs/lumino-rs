//! 高缩放简化层：彩色覆盖光栅（主路径）+ 密度热力图（兜底）。
//!
//! # 彩色覆盖光栅
//!
//! [`coverage`] 把音符矩形压成 `key × tick` 单元，单元内保留最先生效的两条
//! 轨道；渲染时按屏幕像素采样，**保留真实轨道色和音符宽度**。只在
//! `bin_ticks × zoom_x ≤ 2px` 时启用；更高缩放仍走 Exact。它不依赖 30 小节
//! 阈值——<30 小节但已密集到扫不动的区间，也用同一张覆盖图代替逐音符扫描。
//!
//! # 密度热力图
//!
//! 同屏可见小节数 **≥ 30** 时，若覆盖层因基础 bin 过粗不可用，退回蓝色密度
//! 热力图（[`render`]）。30 小节仍是热力图的唯一判定，不使用短音符宽度/数量
//! 等间接阈值。
//!
//! # 共同约定
//!
//! - 透明叠加层：只有非空格子写不透明色，网格/键盘/标尺仍由 UI 绘制；
//! - 物理像素输出：相机 `zoom/scroll` 已含 DPI；
//! - [`overview_note_estimate`] 只做诊断，不参与档位判定。
//!
//! # 内存
//!
//! 热力计数层 `bins × keys × u16`；覆盖层基础 bin = 8 tick，单元 `4B`，单层
//! 最多 32M 单元，逐层 2:1 合并。

use lumino_midi_model::MidiDocument;

use crate::{camera::CanvasCamera, index::TrackIndex};

mod build;
mod coverage;
mod grow;
mod heat;
mod rebuild;
mod render;

use build::{base_plan, count_base};
use coverage::{CoverageLevel, build_coverage};

pub use heat::heat_argb;

/// 基础 bin 的 tick 数下限（与原示例一致）。
const BASE_BIN_TICKS: u32 = 64;
/// 层数：基础桶宽 × `2^0..=2^6`。
const LEVELS: usize = 7;
/// 基础层 bin 数上限：控制内存（`bins × keys × 2B`）。
const MAX_BASE_BINS: usize = 32_768;
/// 同屏可见小节数达到该值时，**热力图兜底**允许启用。
///
/// 彩色覆盖光栅是主路径；只有覆盖层不可用（例如 PPQ 极低导致基础 bin 已
/// 超过 2px）时才看这个阈值。未达 30 小节不得显示蓝色热力图。
pub const OVERVIEW_MIN_MEASURES: u32 = 30;

/// 总览渲染的可复用临时缓冲（避免每帧分配 245k u32 的 key_rows）。
#[derive(Debug, Default)]
pub struct OverviewScratch {
    pub(crate) bin_for_x: Vec<usize>,
    pub(crate) key_rows: Vec<u32>,
}

/// 一层密度数据。
#[derive(Debug)]
struct DensityLevel {
    /// 本层每个 bin 覆盖的 tick 数。
    bin_ticks: u32,
    /// bin 数（≥ 1）。
    bins: usize,
    /// 行优先 `bin × key_count` 的 onset 计数。
    cells: Vec<u16>,
}

/// 密度金字塔（派生聚合数据，不复制音符）。
pub struct DensityPyramid {
    /// 层 0 最细，逐层 2:1 合并。
    levels: Vec<DensityLevel>,
    /// 每层最大计数（颜色归一化用）。
    max_counts: Vec<u16>,
    /// 统计的 key 数（与相机 `key_count` 一致）。
    key_count: u16,
    /// 有效总 tick（文档值或实际最大 end_tick，取大）。
    total_ticks: u32,
    /// 彩色覆盖光栅的 key × tick 单元（按 2:1 分层，保留前两条轨道）。
    coverage: Vec<CoverageLevel>,
    /// 覆盖单元是否仍与文档一致；编辑后置 false，局部重建后恢复。
    coverage_valid: bool,
    /// 待局部重建的 tick 区间 `[lo, hi)`；`None` = 没有脏区。
    coverage_dirty: Option<(u32, u32)>,
}

impl std::fmt::Debug for DensityPyramid {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DensityPyramid")
            .field("levels", &self.levels.len())
            .field("key_count", &self.key_count)
            .field("total_ticks", &self.total_ticks)
            .finish()
    }
}

impl DensityPyramid {
    /// 构建密度计数 + 彩色覆盖金字塔（只读遍历，不复制音符）。
    ///
    /// 计数层按音轨分片并行；覆盖层按轨道顺序单线程写入，保证单元内
    /// first-wins 与 GPU 稳定序一致。`key_count` 必须与相机一致；越界 key
    /// 不参与统计。基础桶宽按 `document.total_ticks` 自适应；若实际音符超出
    /// 容量（例如新建工程 `total_ticks == 0` 后插入音符），按实际最大 tick
    /// 重新规划计数层。
    #[must_use]
    pub fn build(document: &MidiDocument, workers: usize, key_count: u16) -> Self {
        let key_count = key_count.max(1);
        let keys = key_count as usize;
        let mut total_ticks = document.total_ticks;
        let (mut bin_ticks, mut bins) = base_plan(total_ticks);
        let (mut base, max_start, mut max_end) =
            count_base(document, bin_ticks, bins, keys, workers);

        let capacity = u64::from(bin_ticks) * bins as u64;
        if u64::from(max_start) >= capacity || u64::from(max_end) > capacity {
            total_ticks = total_ticks.max(max_start).max(max_end);
            let (next_ticks, next_bins) = base_plan(total_ticks);
            if next_bins > bins || next_ticks != bin_ticks {
                bin_ticks = next_ticks;
                bins = next_bins;
                let (rebuilt, _, rebuilt_end) =
                    count_base(document, bin_ticks, bins, keys, workers);
                base = rebuilt;
                max_end = max_end.max(rebuilt_end);
            }
        }

        let mut levels = Vec::with_capacity(LEVELS);
        levels.push(DensityLevel {
            bin_ticks,
            bins,
            cells: base,
        });
        for _ in 1..LEVELS {
            let Some(previous) = levels.last() else {
                break;
            };
            let next_bins = previous.bins.div_ceil(2).max(1);
            let next_ticks = previous.bin_ticks.saturating_mul(2);
            let mut cells = vec![0u16; next_bins.saturating_mul(keys)];
            for bin in 0..next_bins {
                for key in 0..keys {
                    let first = previous
                        .cells
                        .get((bin * 2) * keys + key)
                        .copied()
                        .unwrap_or(0);
                    let second = previous
                        .cells
                        .get((bin * 2 + 1) * keys + key)
                        .copied()
                        .unwrap_or(0);
                    if let Some(slot) = cells.get_mut(bin * keys + key) {
                        *slot = first.saturating_add(second);
                    }
                }
            }
            levels.push(DensityLevel {
                bin_ticks: next_ticks,
                bins: next_bins,
                cells,
            });
        }

        let max_counts = levels
            .iter()
            .map(|level| level.cells.iter().copied().max().unwrap_or(0).max(1))
            .collect();
        let effective_ticks = total_ticks.max(max_end);
        let coverage = build_coverage(document, effective_ticks, key_count);
        Self {
            levels,
            max_counts,
            key_count,
            total_ticks: effective_ticks,
            coverage,
            coverage_valid: true,
            coverage_dirty: None,
        }
    }

    /// 某层 bin 覆盖的 tick 数。
    #[inline]
    #[must_use]
    pub fn bin_ticks(&self, level: usize) -> u32 {
        self.levels
            .get(level)
            .map_or(BASE_BIN_TICKS, |level| level.bin_ticks)
    }

    /// 某层最大计数（≥ 1）。
    #[inline]
    #[must_use]
    pub fn max_count(&self, level: usize) -> u16 {
        self.max_counts.get(level).copied().unwrap_or(1).max(1)
    }

    /// 统计的 key 数。
    #[inline]
    #[must_use]
    pub fn key_count(&self) -> u16 {
        self.key_count
    }

    /// 彩色覆盖位图是否仍与文档一致（编辑后失效，需整建）。
    #[inline]
    #[must_use]
    pub fn coverage_valid(&self) -> bool {
        self.coverage_valid
    }

    /// 计数层 + 覆盖层占用的字节数（诊断/内存预算用）。
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        let counts: usize = self
            .levels
            .iter()
            .map(|level| level.cells.len() * std::mem::size_of::<u16>())
            .sum();
        let coverage: usize = self
            .coverage
            .iter()
            .map(|level| level.cells.len() * std::mem::size_of::<u32>())
            .sum();
        counts.saturating_add(coverage)
    }

    /// 有效总 tick。
    #[inline]
    #[must_use]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 全曲缩放的层选择：保证 bin 覆盖至少约 1 个屏幕像素。
    #[must_use]
    pub fn level_for_zoom(&self, zoom_x: f32) -> usize {
        if self.levels.is_empty() {
            return 0;
        }
        if !zoom_x.is_finite() || zoom_x <= 0.0 {
            return self.levels.len() - 1;
        }
        let mut level = 0usize;
        while level + 1 < self.levels.len() && (self.levels[level].bin_ticks as f32 * zoom_x) < 1.0
        {
            level += 1;
        }
        level
    }

    /// 编辑后增量更新：`delta = +1` 插入，`-1` 删除。
    ///
    /// 返回 `false` 表示扩容会超过覆盖层安全内存上限，调用方必须整建
    /// （整建会选更粗的 bin）——绝不把音符塞进错误的桶。
    ///
    /// 覆盖层只记录脏 tick 区间，不在这里扫文档；缓存持有者下一次 ensure 时
    /// 调用 [`Self::rebuild_dirty_coverage`] 做局部重建。
    pub fn apply_note(&mut self, start_tick: u32, key: u16, delta: i32) -> bool {
        self.apply_note_range(start_tick, start_tick.saturating_add(1), key, delta)
    }

    /// 带 end_tick 的增量更新；覆盖脏区按 `[start_tick, end_tick)` 标记。
    pub fn apply_note_range(
        &mut self,
        start_tick: u32,
        end_tick: u32,
        key: u16,
        delta: i32,
    ) -> bool {
        if delta == 0 || u32::from(key) >= u32::from(self.key_count) {
            return true;
        }
        let end_tick = end_tick.max(start_tick.saturating_add(1));
        if !grow::ensure_layout(self, start_tick, end_tick) {
            // 增长超过安全内存上限：交回整建（整建会选更粗的 bin）。
            return false;
        }
        self.mark_coverage_dirty(start_tick, end_tick);
        let key = key as usize;
        let keys = self.key_count as usize;
        let magnitude = delta.unsigned_abs().min(u32::from(u16::MAX)) as u16;
        for (index, level) in self.levels.iter_mut().enumerate() {
            let bin = (start_tick / level.bin_ticks.max(1)) as usize;
            let offset = bin.saturating_mul(keys).saturating_add(key);
            let Some(slot) = level.cells.get_mut(offset) else {
                continue;
            };
            if delta > 0 {
                *slot = slot.saturating_add(magnitude);
                if let Some(max) = self.max_counts.get_mut(index) {
                    *max = (*max).max(*slot);
                }
            } else {
                *slot = slot.saturating_sub(magnitude);
            }
        }
        if delta > 0 {
            // 只影响「渲染右界」：长度变化不 rebin，避免一次改尾音就丢增量缓存。
            self.total_ticks = self.total_ticks.max(end_tick);
        }
        true
    }

    /// 只重建编辑脏区覆盖的单元；没有脏区时返回 `false`（调用方应整建）。
    ///
    /// 成本 = 该 tick 区间内的候选音符 + 基础层 2:1 合并，不再扫全文档。
    pub fn rebuild_dirty_coverage(&mut self, document: &MidiDocument, index: &TrackIndex) -> bool {
        coverage::rebuild_dirty_coverage(self, document, index)
    }

    /// 按脏 tick 区间重建计数层与覆盖层（历史回放/大变更）。
    pub fn rebuild_dirty_ranges(
        &mut self,
        document: &MidiDocument,
        index: &TrackIndex,
        ranges: &[(u32, u32)],
    ) -> bool {
        rebuild::rebuild_dirty_ranges(self, document, index, ranges)
    }

    fn mark_coverage_dirty(&mut self, start_tick: u32, end_tick: u32) {
        if self.coverage.is_empty() || end_tick <= start_tick {
            return;
        }
        self.coverage_valid = false;
        self.coverage_dirty = Some(match self.coverage_dirty {
            Some((lo, hi)) => (lo.min(start_tick), hi.max(end_tick)),
            None => (start_tick, end_tick),
        });
    }
}

/// 估算可见音符数（按可见 key 比例缩放小节聚合计数）。
///
/// **仅用于诊断/基准**，不参与总览档位判定。
#[must_use]
pub fn overview_note_estimate(index: &TrackIndex, camera: &CanvasCamera) -> u64 {
    let (tick_start, tick_end) = camera.visible_tick_range();
    let estimate = index.estimate_visible(tick_start, tick_end);
    let (key_low, key_high) = camera.visible_key_range();
    let visible_keys = u64::from(key_high.saturating_sub(key_low).saturating_add(1));
    let total_keys = u64::from(camera.key_count.max(1));
    estimate.saturating_mul(visible_keys) / total_keys
}

/// 屏幕横向覆盖的可见小节数（首尾部分小节都计入）。
///
/// 用 tick 区间与小节边界相交的整数小节数：`[start, end)` 覆盖
/// `start / measure_ticks ..= (end - 1) / measure_ticks`。
#[must_use]
pub fn visible_measure_count(index: &TrackIndex, camera: &CanvasCamera) -> u32 {
    let measure_ticks = index.measure_ticks.max(1);
    let (start, end) = camera.visible_tick_range();
    if end <= start {
        return 0;
    }
    let first = start / measure_ticks;
    let last = end.saturating_sub(1) / measure_ticks;
    last.saturating_sub(first).saturating_add(1)
}

/// 是否应切到总览密度 LOD：**同屏可见小节数 ≥ 30**。
#[must_use]
pub fn uses_overview(index: &TrackIndex, camera: &CanvasCamera) -> bool {
    if index.total_notes == 0 {
        return false;
    }
    if !camera.zoom_x.is_finite()
        || !camera.zoom_y.is_finite()
        || camera.zoom_x <= 0.0
        || camera.zoom_y <= 0.0
    {
        return false;
    }
    visible_measure_count(index, camera) >= OVERVIEW_MIN_MEASURES
}

#[cfg(test)]
#[path = "density/tests.rs"]
mod tests;

#[cfg(test)]
mod tests_grow;
