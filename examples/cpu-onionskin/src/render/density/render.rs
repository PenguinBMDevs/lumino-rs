//! 密度热力图兜底渲染（蓝色 LUT + 逐行缓存）。

use std::sync::LazyLock;

use super::{DensityPyramid, KEY_COUNT, OverviewScratch};
use crate::{
    camera::Camera,
    render::chrome::{COLOR_BG, COLOR_BLACK_ROW, COLOR_RULER_BG, is_black_key},
};

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

pub(super) fn heat_color(count: u16, max_count: u16) -> u32 {
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
    scratch: &mut OverviewScratch,
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

    scratch.bin_for_x.clear();
    scratch.bin_for_x.resize(width, 0);
    for (x, slot) in scratch.bin_for_x.iter_mut().enumerate().skip(keyboard) {
        let tick = camera.x_to_tick(x as f32 + 0.5).max(0.0) as u32;
        *slot = ((tick / bin_ticks) as usize).min(bins - 1);
    }
    if scratch.cached_level != Some(level)
        || scratch.cached_max != max_count
        || scratch.colors.len() != cells.len()
    {
        scratch.colors.clear();
        scratch.colors.reserve(cells.len());
        for &count in cells {
            scratch.colors.push(heat_color(count, max_count));
        }
        scratch.cached_level = Some(level);
        scratch.cached_max = max_count;
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
            let color = scratch
                .colors
                .get(scratch.bin_for_x[x] * KEY_COUNT + key)
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
        let source = &scratch.key_rows[key * width..(key + 1) * width];
        frame[y * width..(y + 1) * width].copy_from_slice(source);
    }
}
