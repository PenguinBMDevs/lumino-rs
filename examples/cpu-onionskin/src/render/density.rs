//! 高缩放简化层：彩色覆盖光栅（主路径）+ 密度热力图（兜底）。
//!
//! 彩色覆盖光栅把音符矩形压成 `key × tick` 单元，保留最先生效的轨道；按屏幕
//! 像素采样后仍是真实轨道色和音符宽度。单元超过 2px 时保持 Exact；它与主程序
//! `lumino-cpu-onion` 的生产优先级一致。
//!
//! 同屏可见小节数 **≥30** 且覆盖层不可用时，退回蓝色密度热力图；30 小节仍是
//! 热力图的唯一判定，不使用短音符宽度/数量等间接阈值。

use std::thread;

use lumino_midi_loader::MidiDocument;

use crate::{camera::Camera, model::TrackIndex};

mod coverage;
mod grow;
mod rebuild;
mod render;

pub use render::render_overview;

/// 同屏可见小节数达到该值时，**热力图兜底**允许启用。
pub const OVERVIEW_MIN_MEASURES: u32 = 30;

/// 基础 bin 的 tick 宽度。
const BASE_BIN_TICKS: u32 = 64;
/// 热力层数（64..4096 tick/bin）。
const LEVELS: usize = 7;
/// MIDI key 数。
const KEY_COUNT: usize = 128;

/// 屏幕横向覆盖的可见小节数（首尾部分小节都计入）。
#[must_use]
pub fn visible_measure_count(index: &TrackIndex, camera: &Camera) -> u32 {
    let measure_ticks = index.measure_ticks.max(1);
    let (start, end) = camera.visible_tick_range();
    if end <= start {
        return 0;
    }
    let first = start / measure_ticks;
    let last = end.saturating_sub(1) / measure_ticks;
    last.saturating_sub(first).saturating_add(1)
}

/// 是否允许热力图兜底：**同屏可见小节数 ≥30**。
#[must_use]
pub fn uses_overview(index: &TrackIndex, camera: &Camera) -> bool {
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

/// 一层热力计数。
#[derive(Debug)]
struct DensityLevel {
    bin_ticks: u32,
    bins: usize,
    cells: Vec<u16>,
}

/// 密度计数 + 彩色覆盖金字塔。
#[derive(Debug)]
pub struct DensityPyramid {
    levels: Vec<DensityLevel>,
    max_counts: Vec<u16>,
    /// 彩色覆盖单元（基础 bin 8 tick，逐层 2:1 合并）。
    coverage: Vec<coverage::CoverageLevel>,
    /// 覆盖层有效总 tick（文档值或实际最大 end_tick 取大）。
    total_ticks: u32,
    /// 编辑后覆盖层失效，下一次需要时局部重建。
    coverage_valid: bool,
    /// 待局部重建的 tick 区间 `[lo, hi)`。
    coverage_dirty: Option<(u32, u32)>,
}

impl DensityPyramid {
    /// 并行统计热力计数；彩色覆盖按轨道顺序单线程构建，保证 first-wins。
    #[must_use]
    pub fn build(document: &MidiDocument, workers: usize) -> Self {
        let total_ticks = u64::from(document.total_ticks());
        let base_bins = ((total_ticks / u64::from(BASE_BIN_TICKS)) as usize).max(1) + 1;
        let track_count = document.track_count();
        let workers = workers.max(1).min(track_count.max(1));
        let chunk = track_count.div_ceil(workers).max(1);
        let mut base = vec![0u16; base_bins * KEY_COUNT];
        let mut max_end = 0u32;

        thread::scope(|scope| {
            let mut handles = Vec::new();
            for worker in 0..workers {
                let start = worker * chunk;
                let end = (start + chunk).min(track_count);
                if start >= end {
                    continue;
                }
                handles.push(scope.spawn(move || {
                    let mut local = vec![0u16; base_bins * KEY_COUNT];
                    let mut local_end = 0u32;
                    for track in start..end {
                        for note in document.track_notes(track).iter() {
                            local_end = local_end.max(note.end_tick);
                            let bin = (note.start_tick / BASE_BIN_TICKS) as usize;
                            if bin >= base_bins {
                                continue;
                            }
                            let index = bin * KEY_COUNT + note.key as usize;
                            local[index] = local[index].saturating_add(1);
                        }
                    }
                    (local, local_end)
                }));
            }
            for handle in handles {
                if let Ok((local, local_end)) = handle.join() {
                    max_end = max_end.max(local_end);
                    for (destination, source) in base.iter_mut().zip(local) {
                        *destination = destination.saturating_add(source);
                    }
                }
            }
        });

        let mut levels = Vec::with_capacity(LEVELS);
        levels.push(DensityLevel {
            bin_ticks: BASE_BIN_TICKS,
            bins: base_bins,
            cells: base,
        });
        for _ in 1..LEVELS {
            let Some(previous) = levels.last() else {
                break;
            };
            let bins = previous.bins.div_ceil(2).max(1);
            let mut cells = vec![0u16; bins * KEY_COUNT];
            for bin in 0..bins {
                for key in 0..KEY_COUNT {
                    let a = previous
                        .cells
                        .get((bin * 2) * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(0);
                    let b = previous
                        .cells
                        .get((bin * 2 + 1) * KEY_COUNT + key)
                        .copied()
                        .unwrap_or(0);
                    cells[bin * KEY_COUNT + key] = a.saturating_add(b);
                }
            }
            levels.push(DensityLevel {
                bin_ticks: BASE_BIN_TICKS << (levels.len() as u32),
                bins,
                cells,
            });
        }
        let max_counts = levels
            .iter()
            .map(|level| level.cells.iter().copied().max().unwrap_or(1).max(1))
            .collect();
        // 新建工程 `total_ticks == 0` 后直接插入音符时，以实际最大 end_tick 为准；
        // 否则覆盖层会把所有列压进最后一个 bin。
        let coverage_ticks = document.total_ticks().max(max_end);
        let coverage = coverage::build_coverage(document, coverage_ticks);
        Self {
            levels,
            max_counts,
            coverage,
            total_ticks: coverage_ticks,
            coverage_valid: true,
            coverage_dirty: None,
        }
    }

    /// 某层热力 bin 宽度（tick）。
    #[inline]
    #[must_use]
    pub fn bin_ticks(&self, level: usize) -> u32 {
        self.levels
            .get(level)
            .map_or(BASE_BIN_TICKS, |level| level.bin_ticks)
    }

    /// 某层最大计数。
    #[inline]
    #[must_use]
    pub fn max_count(&self, level: usize) -> u16 {
        self.max_counts.get(level).copied().unwrap_or(1).max(1)
    }

    /// 热力层选择：保证 bin 至少覆盖 ~1 个屏幕像素。
    #[must_use]
    pub fn level_for_zoom(&self, zoom_x: f32) -> usize {
        let mut level = 0usize;
        while level + 1 < self.levels.len() && (self.bin_ticks(level) as f32 * zoom_x) < 1.0 {
            level += 1;
        }
        level
    }

    /// 彩色覆盖层是否仍与文档一致。
    #[inline]
    #[must_use]
    pub fn coverage_valid(&self) -> bool {
        self.coverage_valid
    }

    /// 覆盖层有效总 tick（渲染右界；目前仅测试断言使用）。
    #[cfg(test)]
    #[inline]
    #[must_use]
    pub fn total_ticks(&self) -> u32 {
        self.total_ticks
    }

    /// 当前缩放是否可能启用彩色覆盖光栅（不检查有效性）。
    #[must_use]
    pub fn coverage_applicable(&self, camera: &Camera) -> bool {
        self.coverage_level_for_zoom(camera.zoom_x).is_some()
    }

    /// 当前缩放是否可实际渲染彩色覆盖光栅。
    #[must_use]
    pub fn can_render_coverage(&self, camera: &Camera) -> bool {
        self.coverage_valid && self.coverage_applicable(camera)
    }

    /// 估算内存（MiB，计数 + 覆盖）。
    #[must_use]
    pub fn memory_mib(&self) -> f64 {
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
        (counts + coverage) as f64 / (1024.0 * 1024.0)
    }

    /// 带 end_tick 的增量更新；覆盖脏区按 `[start_tick, end_tick)` 标记。
    ///
    /// 返回 `false` 表示越过安全内存上限，调用方必须整建。
    pub fn apply_note_range(
        &mut self,
        start_tick: u32,
        end_tick: u32,
        key: u8,
        delta: i32,
    ) -> bool {
        if key as usize >= KEY_COUNT || delta == 0 {
            return true;
        }
        let end_tick = end_tick.max(start_tick.saturating_add(1));
        if !grow::ensure_layout(self, start_tick, end_tick) {
            self.coverage_dirty = None;
            self.coverage_valid = false;
            return false;
        }
        self.coverage_valid = false;
        self.coverage_dirty = Some(match self.coverage_dirty {
            Some((lo, hi)) => (lo.min(start_tick), hi.max(end_tick)),
            None => (start_tick, end_tick),
        });
        for (level_index, level) in self.levels.iter_mut().enumerate() {
            let bin = (start_tick / level.bin_ticks) as usize;
            let Some(cell) = level.cells.get_mut(bin * KEY_COUNT + key as usize) else {
                continue;
            };
            if delta >= 0 {
                *cell = cell.saturating_add(delta as u16);
            } else {
                *cell = cell.saturating_sub((-delta) as u16);
            }
            if delta > 0 {
                self.max_counts[level_index] = self.max_counts[level_index].max(*cell);
            }
        }
        true
    }

    /// 只重建脏区覆盖单元；没有脏区时返回 `false`（调用方应整建）。
    pub fn rebuild_dirty_coverage(&mut self, document: &MidiDocument, index: &TrackIndex) -> bool {
        rebuild::rebuild_dirty_coverage(self, document, index)
    }
}

/// 简化层渲染的可复用临时缓冲。
#[derive(Debug, Default)]
pub struct OverviewScratch {
    pub(crate) bin_for_x: Vec<usize>,
    pub(crate) colors: Vec<u32>,
    pub(crate) key_rows: Vec<u32>,
    pub(crate) cached_level: Option<usize>,
    pub(crate) cached_max: u16,
}

#[cfg(test)]
mod tests;
