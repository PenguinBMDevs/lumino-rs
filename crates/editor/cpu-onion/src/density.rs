//! 全曲总览密度 LOD：极限缩放下把整份工程压成「key × tick 桶」热力图。
//!
//! 从 `examples/cpu-onionskin` 的 Overview 档位移植。生产链路与示例的差异：
//!
//! - **透明叠加层**：背景 alpha = 0，只有密度 > 0 的格子写不透明色；网格、键盘、
//!   标尺仍由 UI 绘制，当前轨仍由 GPU 精确绘制在上层，示例会自己铺背景/标尺。
//! - **统计覆盖全部音轨**：总览缩放级别已无法分辨单轨颜色，且切轨/静音无需重建
//!   金字塔（重建是 O(总音符数)）；与示例的「全曲总览」语义一致。
//! - **物理像素输出**：相机 `scroll/zoom` 已含 DPI 缩放，与网格同口径，不再做
//!   逻辑/物理换算。
//!
//! # 触发条件
//!
//! **按缩放比例与内容长度，不按同屏音符数量**。逐音符绘制只有在音符还有可辨认的
//! 屏幕尺寸时才有意义；当以下任一条件成立时切密度热力图：
//!
//! - 横向：`zoom_x × (measure_ticks / 16) < 2px`（1/16 音符宽度）；
//! - 横向：`zoom_x × short_note_ticks < 2px`（第 10 百分位短音符宽度；
//!   黑乐谱 1-tick 音符在低缩放下逐音符只会退化成整曲扫描）；
//! - 纵向：`zoom_y < 2px`（单键行高）。
//!
//! `measure_ticks` 与 `short_note_ticks` 来自 [`TrackIndex`]，因此阈值随 PPQ
//! 与内容自适应；相机的 `zoom` 已含 DPI，阈值是物理像素。空文档不切换。
//!
//! [`overview_note_estimate`] 仅保留为诊断/基准估算，**不参与档位判定**。
//!
//! # 内存
//!
//! 基础层 `bins × keys × u16`，共 [`LEVELS`] 层（逐层 2:1 合并）；基础桶宽按
//! 总 tick 自适应，bin 数上限 [`MAX_BASE_BINS`]，最坏约 32 MiB（256 keys）。

use std::thread;

use lumino_midi_model::MidiDocument;

use crate::{camera::CanvasCamera, index::TrackIndex};

mod heat;
mod render;

pub use heat::heat_argb;

/// 基础 bin 的 tick 数下限（与原示例一致）。
const BASE_BIN_TICKS: u32 = 64;
/// 层数：基础桶宽 × `2^0..=2^6`。
const LEVELS: usize = 7;
/// 基础层 bin 数上限：控制内存（`bins × keys × 2B`）。
const MAX_BASE_BINS: usize = 32_768;
/// 密度 LOD 的物理像素阈值。
///
/// 1/16 音符宽度或单键行高低于该值时，Exact 路径已经退化成亚像素线条/纯描边；
/// 密度热力图比逐音符堆叠更可读。2px 是「还能看见一个色块」的下限。
pub const OVERVIEW_MIN_NOTE_PX: f32 = 2.0;

/// 短音符（第 10 百分位）仍能占满的最小物理像素宽度。
pub const SHORT_NOTE_MIN_PX: f32 = 2.0;

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
    /// 构建密度金字塔（按音轨分片并行，只读遍历，不复制音符数据）。
    ///
    /// `key_count` 必须与相机一致；越界 key 不参与统计。基础桶宽按
    /// `document.total_ticks` 自适应；若实际音符超出容量（例如新建工程
    /// `total_ticks == 0` 后插入音符），用实际最大 tick 重新规划并重扫一次。
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
        Self {
            levels,
            max_counts,
            key_count,
            total_ticks: total_ticks.max(max_end),
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
    /// 返回 `false` 表示本次变更超出既有 bin 布局（新增 start_tick 越过末 bin），
    /// 调用方必须丢弃金字塔改走整建——绝不把音符塞进错误的桶。
    ///
    /// 删除只做饱和减法、不回收 `max_counts`（颜色略偏暗，下一次整建纠正）；
    /// 与示例一致。
    pub fn apply_note(&mut self, start_tick: u32, key: u16, delta: i32) -> bool {
        if delta == 0 || u32::from(key) >= u32::from(self.key_count) {
            return true;
        }
        let Some(first) = self.levels.first() else {
            return false;
        };
        if (start_tick / first.bin_ticks.max(1)) as usize >= first.bins {
            return false;
        }
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
            self.total_ticks = self.total_ticks.max(start_tick);
        }
        true
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

/// 是否应切到总览密度 LOD（**只看缩放与内容长度，不看音符数量**）。
///
/// 1/16 音符宽度 = `zoom_x × measure_ticks / 16`；`short_note_ticks` 是长度
/// 第 10 百分位。相机 `zoom` 已是物理像素（DPI 已乘入）。
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
    let sixteenth_ticks = (index.measure_ticks / 16).max(1) as f32;
    let short_ticks = index.short_note_ticks.max(1) as f32;
    camera.zoom_x * short_ticks < SHORT_NOTE_MIN_PX
        || camera.zoom_x * sixteenth_ticks < OVERVIEW_MIN_NOTE_PX
        || camera.zoom_y < OVERVIEW_MIN_NOTE_PX
}

/// 基础层规划：桶宽与 bin 数。
fn base_plan(total_ticks: u32) -> (u32, usize) {
    let needed = (u64::from(total_ticks) / MAX_BASE_BINS as u64).saturating_add(1);
    let mut bin_ticks = BASE_BIN_TICKS;
    while u64::from(bin_ticks) < needed {
        bin_ticks = bin_ticks.saturating_mul(2);
    }
    let bins = ((u64::from(total_ticks) / u64::from(bin_ticks)) as usize)
        .saturating_add(1)
        .clamp(1, MAX_BASE_BINS);
    (bin_ticks, bins)
}

/// 并行统计基础层；返回 `(cells, 最大 start_tick, 最大 end_tick)`。
fn count_base(
    document: &MidiDocument,
    bin_ticks: u32,
    bins: usize,
    keys: usize,
    workers: usize,
) -> (Vec<u16>, u32, u32) {
    let stride = bins.saturating_mul(keys);
    let mut base = vec![0u16; stride];
    let track_count = document.notes.len();
    if stride == 0 || track_count == 0 {
        return (base, 0, 0);
    }
    let workers = workers.max(1).min(track_count);
    let chunk = track_count.div_ceil(workers).max(1);
    let mut max_start = 0u32;
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
                let mut local = vec![0u16; stride];
                let mut local_max_start = 0u32;
                let mut local_max_end = 0u32;
                for track in start..end {
                    for note in document.track_notes(track).iter() {
                        local_max_start = local_max_start.max(note.start_tick);
                        local_max_end = local_max_end.max(note.end_tick);
                        if note.key as usize >= keys {
                            continue;
                        }
                        let bin = (note.start_tick / bin_ticks) as usize;
                        if bin >= bins {
                            continue;
                        }
                        let offset = bin * keys + note.key as usize;
                        if let Some(slot) = local.get_mut(offset) {
                            *slot = slot.saturating_add(1);
                        }
                    }
                }
                (local, local_max_start, local_max_end)
            }));
        }
        for handle in handles {
            if let Ok((local, shard_start, shard_end)) = handle.join() {
                max_start = max_start.max(shard_start);
                max_end = max_end.max(shard_end);
                for (destination, source) in base.iter_mut().zip(local) {
                    *destination = destination.saturating_add(source);
                }
            }
        }
    });
    (base, max_start, max_end)
}

#[cfg(test)]
#[path = "density/tests.rs"]
mod tests;
