//! 全曲总览密度金字塔（派生聚合数据，不复制音符）。
//!
//! 层 0 = 64 tick × 1 key 的 onset 计数；层 l = 层 l-1 沿时间轴 2:1 合并。
//! 7 层覆盖 64..4096 tick/bin，总量约 24.8MB（u16）。全曲总览时按屏幕像素
//! 采样对应层，绘制密度热力图；滚动/缩放时仍直接遍历音符（Exact）。

use std::{sync::LazyLock, thread};

use lumino_midi_loader::MidiDocument;

use crate::{
    camera::Camera,
    render::chrome::{COLOR_BG, COLOR_BLACK_ROW, COLOR_RULER_BG, is_black_key},
};

/// 基础 bin 的 tick 宽度。
const BASE_BIN_TICKS: u32 = 64;
/// 层数（64..4096 tick/bin）。
const LEVELS: usize = 7;
/// MIDI key 数。
const KEY_COUNT: usize = 128;

/// 一层密度数据。
#[derive(Debug)]
struct DensityLevel {
    bin_ticks: u32,
    bins: usize,
    cells: Vec<u16>,
}

/// 密度金字塔。
#[derive(Debug)]
pub struct DensityPyramid {
    levels: Vec<DensityLevel>,
    max_counts: Vec<u16>,
}

impl DensityPyramid {
    /// 并行构建（按音轨分片，每线程本地直方图后再合并）。
    #[must_use]
    pub fn build(document: &MidiDocument, workers: usize) -> Self {
        let total_ticks = u64::from(document.total_ticks());
        let base_bins = ((total_ticks / u64::from(BASE_BIN_TICKS)) as usize).max(1) + 1;
        let track_count = document.track_count();
        let workers = workers.max(1).min(track_count.max(1));
        let chunk = track_count.div_ceil(workers).max(1);
        let mut base = vec![0u16; base_bins * KEY_COUNT];

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
                    for track in start..end {
                        for note in document.track_notes(track).iter() {
                            let bin = (note.start_tick / BASE_BIN_TICKS) as usize;
                            if bin >= base_bins {
                                continue;
                            }
                            let index = bin * KEY_COUNT + note.key as usize;
                            local[index] = local[index].saturating_add(1);
                        }
                    }
                    local
                }));
            }
            for handle in handles {
                if let Ok(local) = handle.join() {
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
        Self { levels, max_counts }
    }

    /// 某层 bin 宽度（tick）。
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

    /// 全曲缩放的层选择：保证 bin 至少覆盖 ~1 个屏幕像素。
    #[must_use]
    pub fn level_for_zoom(&self, zoom_x: f32) -> usize {
        let mut level = 0usize;
        while level + 1 < self.levels.len() && (self.bin_ticks(level) as f32 * zoom_x) < 1.0 {
            level += 1;
        }
        level
    }

    /// 估算内存（MiB）。
    #[must_use]
    pub fn memory_mib(&self) -> f64 {
        let bytes: usize = self
            .levels
            .iter()
            .map(|level| level.cells.len() * std::mem::size_of::<u16>())
            .sum();
        bytes as f64 / (1024.0 * 1024.0)
    }

    /// 编辑后增量更新：`delta = +1` 插入，`-1` 删除。
    pub fn apply_note(&mut self, start_tick: u32, key: u8, delta: i32) {
        if key as usize >= KEY_COUNT {
            return;
        }
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
    }
}

/// 热力图色带（sRGB display 空间插值）。
static HEAT_LUT: LazyLock<[u32; 256]> = LazyLock::new(|| {
    const STOPS: [(f32, [u8; 3]); 5] = [
        (0.0, [11, 13, 16]),
        (0.22, [22, 58, 95]),
        (0.48, [42, 111, 219]),
        (0.72, [111, 195, 255]),
        (1.0, [245, 250, 255]),
    ];
    let mut lut = [0u32; 256];
    for (index, slot) in lut.iter_mut().enumerate() {
        let t = index as f32 / 255.0;
        let mut left = STOPS[0];
        let mut right = STOPS[STOPS.len() - 1];
        for pair in STOPS.windows(2) {
            if t >= pair[0].0 && t <= pair[1].0 {
                left = pair[0];
                right = pair[1];
                break;
            }
        }
        let span = (right.0 - left.0).max(0.000_001);
        let local = ((t - left.0) / span).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| -> u8 {
            (f32::from(a) + (f32::from(b) - f32::from(a)) * local).round() as u8
        };
        *slot = ((mix(left.1[0], right.1[0]) as u32) << 16)
            | ((mix(left.1[1], right.1[1]) as u32) << 8)
            | mix(left.1[2], right.1[2]) as u32;
    }
    lut
});

fn heat_color(count: u16, max_count: u16) -> u32 {
    let count = u32::from(count);
    if count == 0 {
        return COLOR_BG;
    }
    let numerator = count.ilog2() as f32;
    let denominator = u32::from(max_count.max(1)).ilog2().max(1) as f32;
    let index = ((numerator / denominator).clamp(0.0, 1.0) * 255.0) as usize;
    HEAT_LUT[index.min(255)]
}

/// 全曲总览：背景 + 密度热力图（直接写窗口 frame）。
pub fn render_overview(
    frame: &mut [u32],
    width: usize,
    height: usize,
    camera: &Camera,
    density: &DensityPyramid,
) {
    let ruler = (camera.ruler_height as usize).min(height);
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

    let level = density.level_for_zoom(camera.zoom_x);
    let bin_ticks = density.bin_ticks(level);
    let max_count = density.max_count(level);
    let Some(level_ref) = density.levels.get(level) else {
        return;
    };
    let bins = level_ref.bins.max(1);
    let cells = &level_ref.cells;
    let keyboard = (camera.keyboard_width as usize).min(width);

    // 逐列 tick → bin 只算一次；逐 cell 颜色也只算一次。原来每像素都做
    // x_to_tick + get + ilog2，1600×900 要 38ms。
    let mut bin_for_x = vec![0usize; width];
    for (x, slot) in bin_for_x.iter_mut().enumerate().skip(keyboard) {
        let tick = camera.x_to_tick(x as f32 + 0.5).max(0.0) as u32;
        *slot = ((tick / bin_ticks) as usize).min(bins - 1);
    }
    let mut colors = Vec::with_capacity(cells.len());
    for &count in cells {
        colors.push(if count == 0 {
            COLOR_BG
        } else {
            heat_color(count, max_count)
        });
    }

    // 每个 key 的整行颜色只算一次：overview 缩放下同一 key 往往覆盖 6~8 行，
    // 逐像素算 1.4M 次不如算 128×width 次再 memcpy。
    let mut key_rows = vec![COLOR_BG; KEY_COUNT * width];
    for key in 0..KEY_COUNT {
        let background = if is_black_key(key as u16) {
            COLOR_BLACK_ROW
        } else {
            COLOR_BG
        };
        let row = &mut key_rows[key * width..(key + 1) * width];
        row.fill(background);
        for (x, pixel) in row.iter_mut().enumerate().skip(keyboard) {
            let color = colors
                .get(bin_for_x[x] * KEY_COUNT + key)
                .copied()
                .unwrap_or(COLOR_BG);
            if color != COLOR_BG {
                *pixel = color;
            }
        }
    }

    for y in ruler..height {
        let key = camera.y_to_key(y as f32 + 0.5) as usize;
        if key >= KEY_COUNT {
            continue;
        }
        let source = &key_rows[key * width..(key + 1) * width];
        frame[y * width..(y + 1) * width].copy_from_slice(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_selection_covers_a_pixel() {
        let pyramid = DensityPyramid {
            levels: (0..LEVELS)
                .map(|index| DensityLevel {
                    bin_ticks: BASE_BIN_TICKS << index,
                    bins: 1,
                    cells: vec![0; KEY_COUNT],
                })
                .collect(),
            max_counts: vec![1; LEVELS],
        };
        assert_eq!(pyramid.level_for_zoom(1.0), 0);
        assert!(pyramid.bin_ticks(pyramid.level_for_zoom(0.0001)) >= 1);
    }

    #[test]
    fn heat_color_zero_is_background() {
        assert_eq!(heat_color(0, 100), COLOR_BG);
        assert_ne!(heat_color(100, 100), COLOR_BG);
    }
}
