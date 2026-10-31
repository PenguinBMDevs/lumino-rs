//! 彩色覆盖光栅：key × tick 单元保留最先生效的轨道。
//!
//! 与蓝色热力图不同，持续音覆盖多少 tick 单元就在屏幕横向连续出多少像素；
//! 单元超过 2px 时退回 Exact。示例没有静音轨，当前轨仍按主轨蓝着色，避免
//! 高缩放时主轨从画面消失。

use lumino_midi_loader::MidiDocument;

use super::{DensityPyramid, KEY_COUNT, OverviewScratch};
use crate::{
    camera::Camera,
    render::{
        chrome::{COLOR_BG, COLOR_BLACK_ROW, COLOR_RULER_BG, is_black_key},
        color::NoteStyle,
    },
};

/// 单层单元数上限（`u16` × 32M ≈ 64 MiB）。
const MAX_COVERAGE_CELLS: usize = 1 << 25;
/// 基础 bin。
const BASE_BIN_TICKS: u32 = 8;
/// 层数：基础 bin × `2^0..=2^10`。
const LEVELS: usize = 11;
/// 空轨道槽。
pub(super) const EMPTY_TRACK: u16 = u16::MAX;
/// 空单元：两条轨道槽都为空。
pub(super) const EMPTY_CELL: u32 = EMPTY_TRACK as u32 | ((EMPTY_TRACK as u32) << 16);

/// 彩色覆盖单元。
#[derive(Debug, Clone)]
pub(super) struct CoverageLevel {
    pub(super) bin_ticks: u32,
    pub(super) bins: usize,
    pub(super) cells: Vec<u32>,
}

impl CoverageLevel {
    #[inline]
    fn get(&self, bin: usize, key: usize) -> u32 {
        self.cells
            .get(bin.saturating_mul(KEY_COUNT).saturating_add(key))
            .copied()
            .unwrap_or(EMPTY_CELL)
    }
}

#[inline]
pub(super) fn cell_first(cell: u32) -> u16 {
    (cell & 0xFFFF) as u16
}

#[inline]
pub(super) fn cell_second(cell: u32) -> u16 {
    (cell >> 16) as u16
}

/// 加入轨道：保留轨道下标最小的两条。
#[inline]
pub(super) fn add_track(cell: &mut u32, track: u16) {
    let first = cell_first(*cell);
    if first == EMPTY_TRACK {
        *cell = u32::from(track) | (u32::from(EMPTY_TRACK) << 16);
        return;
    }
    if first == track {
        return;
    }
    let second = cell_second(*cell);
    if second == EMPTY_TRACK || track < second {
        *cell = u32::from(first) | (u32::from(track) << 16);
    }
}

/// 合并两个单元，保留轨道下标最小的两条。
#[inline]
pub(super) fn merge_cell(a: u32, b: u32) -> u32 {
    let mut candidates = [EMPTY_TRACK; 4];
    let mut len = 0usize;
    for track in [cell_first(a), cell_second(a), cell_first(b), cell_second(b)] {
        if track == EMPTY_TRACK || candidates[..len].contains(&track) {
            continue;
        }
        candidates[len] = track;
        len += 1;
    }
    candidates[..len].sort_unstable();
    let first = candidates[0];
    let second = if len > 1 { candidates[1] } else { EMPTY_TRACK };
    u32::from(first) | (u32::from(second) << 16)
}

/// 按轨道顺序构建；first-wins 必须与 GPU/Exact 的稳定序一致。
pub(super) fn build_coverage(document: &MidiDocument, total_ticks: u32) -> Vec<CoverageLevel> {
    let mut bin_ticks = BASE_BIN_TICKS;
    while (u64::from(total_ticks) / u64::from(bin_ticks))
        .saturating_add(2)
        .saturating_mul(KEY_COUNT as u64)
        > MAX_COVERAGE_CELLS as u64
    {
        bin_ticks = bin_ticks.saturating_mul(2);
        if bin_ticks == u32::MAX {
            break;
        }
    }
    let bins = ((u64::from(total_ticks) / u64::from(bin_ticks)) as usize).saturating_add(2);
    let mut cells = vec![EMPTY_CELL; bins.saturating_mul(KEY_COUNT)];

    for track in 0..document.track_count() {
        let Ok(track_id) = u16::try_from(track) else {
            continue;
        };
        for note in document.track_notes(track).iter() {
            if note.end_tick <= note.start_tick || note.key as usize >= KEY_COUNT {
                continue;
            }
            let first = (note.start_tick / bin_ticks) as usize;
            let last = (note.end_tick.saturating_sub(1) / bin_ticks) as usize;
            let last = last.min(bins.saturating_sub(1));
            if first >= bins || first > last {
                continue;
            }
            for bin in first..=last {
                let index = bin
                    .saturating_mul(KEY_COUNT)
                    .saturating_add(note.key as usize);
                if let Some(cell) = cells.get_mut(index) {
                    add_track(cell, track_id);
                }
            }
        }
    }

    build_levels_from_base(
        CoverageLevel {
            bin_ticks,
            bins,
            cells,
        },
        KEY_COUNT,
    )
}

/// 从基础层重建 2:1 金字塔（构建与脏区局部重建共用）。
pub(super) fn build_levels_from_base(base: CoverageLevel, keys: usize) -> Vec<CoverageLevel> {
    let mut levels = Vec::with_capacity(LEVELS);
    levels.push(base);
    while levels.len() < LEVELS {
        let Some(previous) = levels.last() else {
            break;
        };
        if previous.bins <= 1 {
            break;
        }
        let next_bins = previous.bins.div_ceil(2);
        let mut next_cells = vec![EMPTY_CELL; next_bins.saturating_mul(keys)];
        for bin in 0..next_bins {
            for key in 0..keys {
                let cell = merge_cell(previous.get(bin * 2, key), previous.get(bin * 2 + 1, key));
                let index = bin.saturating_mul(keys).saturating_add(key);
                if let Some(slot) = next_cells.get_mut(index) {
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
    /// 选最粗、且 `bin_ticks × zoom_x ≤ 2px` 的层。
    pub(super) fn coverage_level_for_zoom(&self, zoom_x: f32) -> Option<usize> {
        if !zoom_x.is_finite() || zoom_x <= 0.0 || self.coverage.is_empty() {
            return None;
        }
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

    /// 绘制彩色覆盖光栅（背景 + 真实轨道色，直接写窗口 frame）。
    ///
    /// 当前轨使用主轨蓝；没有静音概念，所有轨道都参与 first-wins。
    #[allow(clippy::too_many_arguments)]
    pub fn render_coverage(
        &self,
        frame: &mut [u32],
        width: usize,
        height: usize,
        camera: &Camera,
        style: &NoteStyle,
        current_track: usize,
        scratch: &mut OverviewScratch,
    ) {
        if width == 0 || height == 0 || frame.len() < width * height {
            return;
        }
        let Some(level) = self.coverage_level_for_zoom(camera.zoom_x) else {
            return;
        };
        let Some(data) = self.coverage.get(level) else {
            return;
        };
        let ruler = (camera.ruler_height.round().max(0.0) as usize).min(height);
        for y in 0..height {
            let color = if y < ruler {
                COLOR_RULER_BG
            } else {
                let key = camera.y_to_key(y as f32 + 0.5);
                if is_black_key(key) {
                    COLOR_BLACK_ROW
                } else {
                    COLOR_BG
                }
            };
            frame[y * width..(y + 1) * width].fill(color);
        }
        if ruler > 0 {
            frame[(ruler - 1) * width..ruler * width].fill(0x3A_414D);
        }

        let keyboard = (camera.keyboard_width.round().max(0.0) as usize).min(width);
        let bins = data.bins.max(1);
        let bin_ticks = u64::from(data.bin_ticks.max(1));
        scratch.bin_for_x.clear();
        scratch.bin_for_x.resize(width, usize::MAX);
        for (x, slot) in scratch.bin_for_x.iter_mut().enumerate().skip(keyboard) {
            let tick = camera.x_to_tick(x as f32 + 0.5).max(0.0);
            if !tick.is_finite() || tick > self.total_ticks as f32 {
                continue;
            }
            let bin = (tick as u64 / bin_ticks) as usize;
            *slot = bin.min(bins - 1);
        }

        scratch.key_rows.clear();
        scratch.key_rows.resize(KEY_COUNT * width, COLOR_BG);
        for key in 0..KEY_COUNT {
            let background = if is_black_key(key as u16) {
                COLOR_BLACK_ROW
            } else {
                COLOR_BG
            };
            let row = &mut scratch.key_rows[key * width..(key + 1) * width];
            row.fill(background);
            for (x, pixel) in row.iter_mut().enumerate().skip(keyboard) {
                let bin = scratch.bin_for_x[x];
                if bin == usize::MAX {
                    continue;
                }
                let cell = data.get(bin, key);
                let first = cell_first(cell);
                if first == EMPTY_TRACK {
                    continue;
                }
                let second = cell_second(cell);
                let track = if first as usize == current_track {
                    first
                } else if second as usize == current_track {
                    second
                } else {
                    first
                };
                *pixel = style.fill(track as usize, track as usize == current_track);
            }
        }

        for y in ruler..height {
            let key = camera.y_to_key(y as f32 + 0.5) as usize;
            if key >= KEY_COUNT {
                continue;
            }
            let source = &scratch.key_rows[key * width..(key + 1) * width];
            frame[y * width..(y + 1) * width].copy_from_slice(source);
        }
    }
}
