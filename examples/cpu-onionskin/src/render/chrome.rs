//! 网格 / 键盘 / 标尺 / HUD —— 卷帘的非音符层。
//!
//! 颜色是示例自己的暗色主题；音符层严格复用 `color.rs` 的 GPU 口径。

use crate::{
    camera::Camera,
    render::{
        buffer::BandBuffer,
        simd::{SimdLevel, fill_row},
    },
    text::{TextRenderer, draw_hud},
};

pub const COLOR_BG: u32 = 0x0B_0D_10;
pub const COLOR_BLACK_ROW: u32 = 0x11_14_1A;
pub const COLOR_RULER_BG: u32 = 0x0E_10_14;
pub const COLOR_BAR_LINE: u32 = 0x39_40_4C;
pub const COLOR_BEAT_LINE: u32 = 0x22_28_33;
pub const COLOR_KEY_LINE: u32 = 0x1A_1F_27;
pub const COLOR_WHITE_KEY: u32 = 0xC8_CED8;
pub const COLOR_BLACK_KEY: u32 = 0x1B_1F_26;
pub const COLOR_KEY_BORDER: u32 = 0x2A_30_3A;
pub const COLOR_KEYBOARD_EDGE: u32 = 0x3A_414D;
pub const COLOR_TEXT: u32 = 0xD6_DCE6;
pub const COLOR_HUD_BG: u32 = 0x10_13_18;

/// 黑键判定（与 `infinite_grid.wgsl` 的 `is_black_key` 同口径）。
#[inline]
#[must_use]
pub fn is_black_key(key: u16) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

/// 背景 + 横向键线 + 纵向小节/拍线 + 标尺底边。
///
/// `band_x0` 是 band 在窗口中的起始 x；所有绘制都换算成 band 局部坐标。
pub fn render_background(
    buffer: &mut BandBuffer,
    camera: &Camera,
    band_x0: u32,
    simd: SimdLevel,
    draw_grid: bool,
) {
    let width = buffer.width;
    let height = buffer.height;
    let ruler = camera.ruler_height as usize;

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
        let row = buffer.row(y);
        fill_row(row, 0, width as i32, color, simd);
    }

    // 横向键边界线。
    if camera.zoom_y >= 3.0 {
        let (key_lo, key_hi) = camera.visible_key_range();
        for key in key_lo..=key_hi.saturating_add(1).min(camera.key_count - 1) {
            let y = camera.key_to_y(key);
            let y = y.round() as i32;
            if y > ruler as i32 && y < height as i32 {
                let row = buffer.row(y as usize);
                fill_row(row, 0, width as i32, COLOR_KEY_LINE, simd);
            }
        }
    }

    if draw_grid {
        draw_vertical_grid(buffer, camera, band_x0, simd);
    }

    // 标尺底边。
    if ruler > 0 && ruler <= height {
        let row = buffer.row(ruler - 1);
        fill_row(row, 0, width as i32, COLOR_KEYBOARD_EDGE, simd);
    }
}

fn draw_vertical_grid(buffer: &mut BandBuffer, camera: &Camera, band_x0: u32, simd: SimdLevel) {
    let ppq = camera.ppq as u32;
    if ppq == 0 {
        return;
    }
    let beat_ticks = ppq;
    let measure_ticks = ppq.saturating_mul(4);
    let (tick_start, tick_end) = camera.visible_tick_range();
    let ruler = camera.ruler_height as i32;
    let height = buffer.height as i32;
    let width = buffer.width as i32;

    // 小节线（2px）。
    let measure_px = measure_ticks as f32 * camera.zoom_x;
    if measure_px >= 3.0 {
        let first = (tick_start / measure_ticks) as u64;
        let last = (tick_end / measure_ticks) as u64;
        for measure in first..=last {
            let tick = measure.saturating_mul(measure_ticks as u64) as u32;
            draw_vertical_line(
                buffer,
                camera.tick_to_x(tick as f32) - band_x0 as f32,
                width,
                ruler,
                height,
                2,
                COLOR_BAR_LINE,
            );
        }
    }

    // 拍线（1px）：只在间距足够时画。
    let beat_px = beat_ticks as f32 * camera.zoom_x;
    if beat_px >= 9.0 {
        let first = (tick_start / beat_ticks) as u64;
        let last = (tick_end / beat_ticks) as u64;
        for beat in first..=last {
            let tick = beat.saturating_mul(beat_ticks as u64) as u32;
            if tick.is_multiple_of(measure_ticks) {
                continue;
            }
            draw_vertical_line(
                buffer,
                camera.tick_to_x(tick as f32) - band_x0 as f32,
                width,
                ruler,
                height,
                1,
                COLOR_BEAT_LINE,
            );
        }
    }

    // 标尺内的小节刻度。
    if measure_px >= 12.0 {
        let first = (tick_start / measure_ticks) as u64;
        let last = (tick_end / measure_ticks) as u64;
        for measure in first..=last {
            let tick = measure.saturating_mul(measure_ticks as u64) as u32;
            let x = (camera.tick_to_x(tick as f32) - band_x0 as f32).round() as i32;
            if x >= 0 && x < width {
                for y in (ruler - 8).max(0)..ruler {
                    let index = y as usize * width as usize + x as usize;
                    buffer.pixels[index] = COLOR_BEAT_LINE;
                }
            }
        }
    }

    let _ = simd;
}

fn draw_vertical_line(
    buffer: &mut BandBuffer,
    x: f32,
    width: i32,
    y0: i32,
    y1: i32,
    thickness: i32,
    color: u32,
) {
    let x = x.round() as i32;
    if x + thickness <= 0 || x >= width {
        return;
    }
    let start = x.max(0);
    let end = (x + thickness).min(width);
    for y in y0.max(0)..y1 {
        let row = y as usize * buffer.width;
        for px in start..end {
            buffer.pixels[row + px as usize] = color;
        }
    }
}

/// 左侧键盘（垂直滚动跟随）。
pub fn draw_keyboard(
    frame: &mut [u32],
    stride: usize,
    camera: &Camera,
    font: Option<&TextRenderer>,
) {
    let kb = camera.keyboard_width.round() as usize;
    if kb == 0 || stride == 0 {
        return;
    }
    let height = frame.len() / stride;
    let font_size = (11.0 * camera.scale_factor).max(8.0);

    for y in 0..height {
        let color = if y < camera.ruler_height as usize {
            COLOR_RULER_BG
        } else {
            let key = camera.y_to_key(y as f32 + 0.5);
            if is_black_key(key) {
                COLOR_BLACK_KEY
            } else {
                COLOR_WHITE_KEY
            }
        };
        for x in 0..kb {
            frame[y * stride + x] = color;
        }
    }

    // 键分隔线 + C 标签。
    let (key_lo, key_hi) = camera.visible_key_range();
    for key in key_lo..=key_hi {
        let top = camera.key_to_y(key).round() as i32;
        let bottom = (camera.key_to_y(key) + camera.zoom_y).round() as i32;
        if bottom <= 0 || top >= height as i32 {
            continue;
        }
        let line_y = bottom - 1;
        if line_y >= 0 && line_y < height as i32 {
            for x in 0..kb {
                frame[line_y as usize * stride + x] = COLOR_KEY_BORDER;
            }
        }
        if key % 12 == 0
            && let Some(font) = font
        {
            let octave = key as i32 / 12 - 1;
            let label = format!("C{octave}");
            let text_y = (top as f32 + (camera.zoom_y - font_size) * 0.5)
                .clamp(camera.ruler_height, height as f32 - font_size);
            font.draw(frame, stride, 6.0, text_y, &label, COLOR_TEXT, font_size);
        }
    }

    // 键盘右边界。
    let edge = kb.saturating_sub(1);
    for y in 0..height {
        frame[y * stride + edge] = COLOR_KEYBOARD_EDGE;
    }
}

/// 重绘整条标尺（背景 + 小节刻度 + 小节号）。
///
/// 滚动增量路径按像素平移已有帧后，标尺上的旧小节号会留在错误位置；本函数
/// 用很小的代价把标尺区域整体清掉重画，避免文字拖影。
pub fn draw_ruler(frame: &mut [u32], stride: usize, camera: &Camera, font: Option<&TextRenderer>) {
    let height = frame.len() / stride.max(1);
    let ruler = (camera.ruler_height as usize).min(height);
    let kb = (camera.keyboard_width as usize).min(stride);
    if ruler == 0 || kb >= stride {
        return;
    }
    for y in 0..ruler {
        frame[y * stride + kb..(y + 1) * stride].fill(COLOR_RULER_BG);
    }
    frame[(ruler - 1) * stride + kb..ruler * stride].fill(COLOR_KEYBOARD_EDGE);

    let measure_ticks = (camera.ppq as u32).saturating_mul(4).max(1);
    let (tick_start, tick_end) = camera.visible_tick_range();
    let first = (tick_start / measure_ticks) as u64;
    let last = (tick_end / measure_ticks) as u64;
    let spacing = measure_ticks as f32 * camera.zoom_x;
    if spacing >= 12.0 {
        for measure in first..=last {
            let tick = measure.saturating_mul(measure_ticks as u64) as u32;
            let x = camera.tick_to_x(tick as f32).round() as i32;
            if x < kb as i32 || x >= stride as i32 {
                continue;
            }
            let x = x as usize;
            for y in ruler.saturating_sub(8)..ruler {
                frame[y * stride + x] = COLOR_BEAT_LINE;
            }
        }
    }
    draw_ruler_labels(frame, stride, camera, font);
}

/// 标尺上的小节号。
pub fn draw_ruler_labels(
    frame: &mut [u32],
    stride: usize,
    camera: &Camera,
    font: Option<&TextRenderer>,
) {
    let Some(font) = font else {
        return;
    };
    let measure_ticks = (camera.ppq as u32).saturating_mul(4).max(1);
    let (tick_start, tick_end) = camera.visible_tick_range();
    let first = (tick_start / measure_ticks) as u64;
    let last = (tick_end / measure_ticks) as u64;
    let spacing = measure_ticks as f32 * camera.zoom_x;
    let step = ((44.0 / spacing.max(1.0)).ceil() as u64).max(1);
    let size = (12.0 * camera.scale_factor).max(9.0);

    for measure in (first..=last).step_by(step as usize) {
        let tick = measure.saturating_mul(measure_ticks as u64) as u32;
        let x = camera.tick_to_x(tick as f32);
        if x < camera.keyboard_width || x > camera.width as f32 - 30.0 {
            continue;
        }
        let label = (measure + 1).to_string();
        font.draw(frame, stride, x + 4.0, 2.0, &label, COLOR_TEXT, size);
    }
}

/// 右上角 HUD：LOD / 可见音符 / 帧率 / 帧时间。
pub fn draw_status(
    frame: &mut [u32],
    stride: usize,
    camera: &Camera,
    text: &str,
    font: Option<&TextRenderer>,
) {
    let Some(font) = font else {
        return;
    };
    let size = 13.0;
    let width = font.width(text, size) + 12.0;
    let x = (camera.width as f32 - width - 8.0).max(camera.keyboard_width + 8.0);
    draw_hud(
        frame,
        stride,
        x,
        3.0,
        text,
        COLOR_TEXT,
        COLOR_HUD_BG,
        Some(font),
    );
}
