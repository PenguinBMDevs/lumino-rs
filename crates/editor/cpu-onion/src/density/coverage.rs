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
//! 只在 `bin_ticks × zoom_x ≤ 1` 时启用；更高缩放仍走 Exact 逐音符光栅。

use lumino_midi_model::MidiDocument;

use super::DensityPyramid;
use crate::{camera::CanvasCamera, color::NoteStyle};

/// 单层单元数上限（约 32 MiB 的 `u32`）。
const MAX_COVERAGE_CELLS: usize = 1 << 25;
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
    const EMPTY: Self = Self(EMPTY_TRACK as u32 | ((EMPTY_TRACK as u32) << 16));

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
    fn add(&mut self, track: u16) {
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
    fn merge(self, other: Self) -> Self {
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
    fn get(&self, bin: usize, key: usize, keys: usize) -> CoverageCell {
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

    let mut levels = Vec::with_capacity(LEVELS);
    levels.push(CoverageLevel {
        bin_ticks,
        bins,
        cells,
    });
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

impl DensityPyramid {
    /// 选择最粗、且 `bin_ticks × zoom_x ≤ 1` 的层；不存在时返回 `None`。
    fn coverage_level_for_zoom(&self, zoom_x: f32) -> Option<usize> {
        if !zoom_x.is_finite() || zoom_x <= 0.0 || self.coverage.is_empty() {
            return None;
        }
        // 允许一个 cell 覆盖到 2×2 屏幕像素：这是 100fps 与可读性的折中。
        // 超过 2px 的块会明显吃掉音符宽度，退回 Exact。
        const MAX_CELL_PX: f32 = 2.0;
        let mut level = 0usize;
        for next in 1..self.coverage.len() {
            if self.coverage[next].bin_ticks as f32 * zoom_x <= MAX_CELL_PX {
                level = next;
            } else {
                break;
            }
        }
        let selected = self.coverage.get(level)?;
        (selected.bin_ticks as f32 * zoom_x <= MAX_CELL_PX).then_some(level)
    }

    /// 彩色覆盖光栅是否可用于当前缩放。
    #[must_use]
    pub fn can_render_coverage(&self, camera: &CanvasCamera) -> bool {
        self.coverage_valid && self.coverage_level_for_zoom(camera.zoom_x).is_some()
    }

    /// 把彩色覆盖光栅写进内部 `0xAARRGGBB` 帧（透明背景）。
    ///
    /// `current_track` / `muted` 参与候选轨道选择：单元第一条被排除时取第二条。
    /// 只有两条都被排除才留空；这个分辨率下第三条与第二条同像素，差异不可见。
    #[allow(clippy::too_many_arguments)]
    pub fn render_coverage_with_scratch(
        &self,
        frame: &mut [u32],
        width: u32,
        height: u32,
        camera: &CanvasCamera,
        style: &NoteStyle,
        current_track: usize,
        muted: &[bool],
        scratch: &mut super::OverviewScratch,
    ) {
        let width = width as usize;
        let height = height as usize;
        if width == 0
            || height == 0
            || self.key_count == 0
            || frame.len() != width.saturating_mul(height)
        {
            return;
        }
        let Some(level) = self.coverage_level_for_zoom(camera.zoom_x) else {
            return;
        };
        let Some(data) = self.coverage.get(level) else {
            return;
        };
        let keys = self.key_count as usize;
        let bins = data.bins.max(1);
        let bin_ticks = u64::from(data.bin_ticks.max(1));
        let total_ticks = f64::from(self.total_ticks);

        scratch.bin_for_x.clear();
        scratch.bin_for_x.resize(width, usize::MAX);
        for (x, slot) in scratch.bin_for_x.iter_mut().enumerate() {
            let tick = camera.x_to_tick(x as f32 + 0.5);
            if !tick.is_finite() || tick < 0.0 || f64::from(tick) > total_ticks {
                continue;
            }
            let bin = (tick as u64 / bin_ticks) as usize;
            *slot = bin.min(bins - 1);
        }

        let allowed = |track: u16| -> bool {
            let track = track as usize;
            track != current_track && !muted.get(track).copied().unwrap_or(false)
        };
        let row_len = keys.saturating_mul(width);
        scratch.key_rows.clear();
        scratch.key_rows.resize(row_len, 0);
        for key in 0..keys {
            let Some(row) = scratch.key_rows.get_mut(key * width..(key + 1) * width) else {
                continue;
            };
            for (x, pixel) in row.iter_mut().enumerate() {
                let bin = scratch.bin_for_x[x];
                if bin == usize::MAX {
                    continue;
                }
                let cell = data.get(bin, key, keys);
                if cell.is_empty() {
                    continue;
                }
                let second = cell.second();
                let track = if allowed(cell.first()) {
                    cell.first()
                } else if second != EMPTY_TRACK && allowed(second) {
                    second
                } else {
                    continue;
                };
                *pixel = style.fill(track as usize);
            }
        }

        let max_key = camera.max_key() as f32;
        for y in 0..height {
            let key = camera.y_to_key_f32(y as f32 + 0.5).ceil();
            let in_range = key.is_finite() && key >= 0.0 && key <= max_key;
            let key = key as usize;
            let Some(destination) = frame.get_mut(y * width..(y + 1) * width) else {
                continue;
            };
            if !in_range {
                destination.fill(0);
                continue;
            }
            let Some(source) = scratch.key_rows.get(key * width..(key + 1) * width) else {
                destination.fill(0);
                continue;
            };
            destination.copy_from_slice(source);
        }
    }
}
