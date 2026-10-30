//! 单音符矩形光栅化（全质量填充 + 每侧 2px 描边，无抗锯齿）。
//!
//! 几何与 GPU `onion_note.wgsl` 的 quad 路径一致：
//! - 屏幕矩形 `[sx, sx + sw) × [sy, sy + sh)`，像素中心覆盖规则
//!   `sx <= i + 0.5 < sx + sw`；
//! - 边框 = 着色器 `horiz_margin = border_px / (width * 0.5)` 的等价像素域：
//!   每侧 `2 × border_width = 2px`；
//! - 填充/边框同属一个音符实例，共用 owner 掩码，重叠时先到先得。
//!
//! # 填充边界的等号归属（与着色器逐像素对齐）
//!
//! 着色器用**严格不等**判边框：左/上是 `uv < margin`，右/下是 `uv > 1 - margin`。
//! 于是填充边界**恰好落在某像素中心**时，四边的等号都归**填充**。本文件因此区分
//! 两个取整函数（[`fill_start`] / [`fill_end`]），而不是四边共用一个式子：
//! 四边同式会让右/下在等号处多数出一列/一行深色描边，在 125%/150% 这类半像素
//! 几何下与 GPU 画面出现 1px 差异（Step 6b 全像素对比抓到的 48/36 处）。

use lumino_midi_model::NoteEvent;

use crate::{
    band::BandJob,
    buffer::BandBuffer,
    color::NOTE_BORDER_PX,
    simd::{SimdLevel, masked_fill_row},
};

/// 左/上界：第一个「像素中心 ≥ `edge`」的列/行（填充区**闭**起点）。
///
/// 与着色器 `uv < margin`（严格小于）在等号处判填充同义：像素 `i` 的中心为
/// `i + 0.5`，填充起点 = 最小的 `i` 使 `i + 0.5 >= edge`，即 `ceil(edge - 0.5)`。
#[inline]
fn fill_start(edge: f32) -> i32 {
    (edge - 0.5).ceil() as i32
}

/// 右/下界：填充区的**排他**终点（该列/行起为描边）。
///
/// 与着色器 `uv > 1 - margin`（严格大于）在等号处判填充同义：像素 `i` 参与填充
/// ⇔ `i + 0.5 <= edge`，故终点 = 第一个 `i + 0.5 > edge` 的 `i`
/// = `floor(edge - 0.5) + 1`。
///
/// 与旧式（四边共用 `ceil(edge - 0.5)`）的差异**只出现在 `edge` 恰为半整数**时：
/// 半个像素中心正好落在边界上，旧式判描边、着色器判填充。非半整数时两式恒等
/// （`ceil(x) == floor(x) + 1`，`x = edge - 0.5` 非整数）。
///
/// 负值 / NaN / 无穷：`as i32` 为饱和转换（NaN → 0），不 panic。
#[inline]
fn fill_end(edge: f32) -> i32 {
    ((edge - 0.5).floor() + 1.0) as i32
}

/// 画一条音符；矩形与 band/视口无交集时返回 `false`。
pub(crate) fn draw_note(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    note: &NoteEvent,
) -> Option<usize> {
    let camera = &job.camera;
    let zoom_x = camera.zoom_x;
    let sx = note.start_tick as f32 * zoom_x - camera.scroll_x;
    let sw = note.length() as f32 * zoom_x;
    if sw <= 0.0 {
        return None;
    }
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    if sh <= 0.0 {
        return None;
    }

    let mut ix0 = fill_start(sx);
    let mut ix1 = fill_start(sx + sw);
    let mut iy0 = fill_start(sy);
    let mut iy1 = fill_start(sy + sh);
    ix0 = ix0.max(job.x0 as i32);
    ix1 = ix1.min(job.x1 as i32);
    iy0 = iy0.max(0);
    iy1 = iy1.min(camera.height as i32);
    if ix1 <= ix0 || iy1 <= iy0 {
        return None;
    }

    let fill = job.style.fill(track);
    let border = job.style.border(track);
    // 洋葱皮 owner 固定为 1；owner_limit = 0 表示只允许未被占用的像素。
    let (owner_value, owner_limit) = (1u32, 0u32);

    // 每侧实际边框 = 2 × border_width = 2px。填充分界（浮点）。
    let border_px = NOTE_BORDER_PX;
    let fx0_f = sx + border_px;
    let fx1_f = sx + sw - border_px;
    let fy0_f = sy + border_px;
    let fy1_f = sy + sh - border_px;
    let mut fx0 = fill_start(fx0_f);
    let mut fx1 = fill_end(fx1_f);
    let mut fy0 = fill_start(fy0_f);
    let mut fy1 = fill_end(fy1_f);
    fx0 = fx0.max(ix0);
    fx1 = fx1.min(ix1);
    fy0 = fy0.max(iy0);
    fy1 = fy1.min(iy1);
    let has_fill = fx1 > fx0 && fy1 > fy0;

    let mut covered = 0usize;

    if !has_fill {
        // 细音符 / 纯描边：宽度 ≤ 8px 时直接遍历像素，避免每行一次函数调用。
        let width_px = ix1 - ix0;
        if width_px == 1 {
            covered += draw_pixel_column(
                buffer,
                ix0 - job.x0 as i32,
                iy0,
                iy1,
                border,
                owner_value,
                owner_limit,
            );
        } else if width_px <= 8 {
            covered += draw_owner_block(
                buffer,
                job.x0,
                ix0,
                ix1,
                iy0,
                iy1,
                border,
                owner_value,
                owner_limit,
            );
        } else {
            for y in iy0..iy1 {
                covered += draw_segment(
                    buffer,
                    y,
                    job.x0,
                    ix0,
                    ix1,
                    border,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
        }
        return Some(covered);
    }

    for y in iy0..iy1 {
        if y >= fy0 && y < fy1 {
            if fx0 > ix0 {
                covered += draw_segment(
                    buffer,
                    y,
                    job.x0,
                    ix0,
                    fx0,
                    border,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
            covered += draw_segment(
                buffer,
                y,
                job.x0,
                fx0,
                fx1,
                fill,
                owner_value,
                owner_limit,
                job.simd,
            );
            if ix1 > fx1 {
                covered += draw_segment(
                    buffer,
                    y,
                    job.x0,
                    fx1,
                    ix1,
                    border,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
        } else {
            covered += draw_segment(
                buffer,
                y,
                job.x0,
                ix0,
                ix1,
                border,
                owner_value,
                owner_limit,
                job.simd,
            );
        }
    }
    Some(covered)
}

#[allow(clippy::too_many_arguments)]
fn draw_owner_block(
    buffer: &mut BandBuffer,
    band_x0: u32,
    x0: i32,
    x1: i32,
    y0: i32,
    y1: i32,
    color: u32,
    owner_value: u32,
    owner_limit: u32,
) -> usize {
    let lx0 = (x0 - band_x0 as i32).max(0) as usize;
    let lx1 = (x1 - band_x0 as i32).min(buffer.width as i32).max(0) as usize;
    if lx1 <= lx0 {
        return 0;
    }
    let width = buffer.width;
    let start_y = y0.max(0) as usize;
    let end_y = (y1.min(buffer.height as i32)).max(0) as usize;
    let mut covered = 0usize;
    for y in start_y..end_y {
        let base = y * width;
        for x in lx0..lx1 {
            let index = base + x;
            if buffer.owner[index] <= owner_limit {
                if buffer.owner[index] == 0 {
                    covered += 1;
                }
                buffer.pixels[index] = color;
                buffer.owner[index] = owner_value;
            }
        }
    }
    covered
}

#[allow(clippy::too_many_arguments)]
fn draw_segment(
    buffer: &mut BandBuffer,
    y: i32,
    band_x0: u32,
    x0: i32,
    x1: i32,
    color: u32,
    owner_value: u32,
    owner_limit: u32,
    simd: SimdLevel,
) -> usize {
    let lx0 = x0 - band_x0 as i32;
    let lx1 = x1 - band_x0 as i32;
    if lx1 <= lx0 {
        return 0;
    }
    let width = buffer.width;
    let row_start = y as usize * width;
    let (pixels, owner) = (&mut buffer.pixels, &mut buffer.owner);
    let row = &mut pixels[row_start..row_start + width];
    let own = &mut owner[row_start..row_start + width];
    masked_fill_row(row, own, lx0, lx1, color, owner_value, owner_limit, simd)
}

#[allow(clippy::too_many_arguments)]
fn draw_pixel_column(
    buffer: &mut BandBuffer,
    x: i32,
    y0: i32,
    y1: i32,
    color: u32,
    owner_value: u32,
    owner_limit: u32,
) -> usize {
    if x < 0 || x >= buffer.width as i32 {
        return 0;
    }
    let x = x as usize;
    let mut covered = 0usize;
    for y in y0..y1 {
        let index = y as usize * buffer.width + x;
        if buffer.owner[index] <= owner_limit {
            if buffer.owner[index] == 0 {
                covered += 1;
            }
            buffer.pixels[index] = color;
            buffer.owner[index] = owner_value;
        }
    }
    covered
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 半整数边界：中心恰落在填充边界上的像素必须算**填充**（对齐着色器 `>`）。
    #[test]
    fn fill_end_treats_half_integer_edge_as_fill() {
        // edge = 10.5：像素 10 的中心 10.5 正好在边界上 → 填充（终点排他 = 11）
        assert_eq!(fill_end(10.5), 11);
        // 非半整数：与 ceil(edge - 0.5) 恒等
        assert_eq!(fill_end(10.25), 10);
        assert_eq!(fill_end(10.0), 10);
        assert_eq!(fill_end(10.75), 11);
        assert_eq!(fill_end(10.6), 11);
        // 左/上界（等号归填充）不受影响
        assert_eq!(fill_start(10.5), 10);
        assert_eq!(fill_start(10.0), 10);
        assert_eq!(fill_start(10.25), 10);
        assert_eq!(fill_start(10.75), 11);
    }

    /// 两式只在半整数处分歧；逐 1/4 像素扫描锁死这条边界。
    #[test]
    fn fill_end_differs_from_ceil_only_on_half_integers() {
        for quarters in 0..(40 * 4) {
            let edge = quarters as f32 * 0.25;
            let old = (edge - 0.5).ceil() as i32;
            let new = fill_end(edge);
            if (edge - 0.5).fract() == 0.0 {
                assert_eq!(
                    new,
                    old + 1,
                    "edge={edge}：半整数边界必须把等号像素算作填充（旧 {old} → 新 {new}）"
                );
            } else {
                assert_eq!(new, old, "edge={edge}：非半整数时两式必须恒等");
            }
        }
    }

    /// 负值 / NaN / 无穷不 panic（饱和转换）。
    #[test]
    fn fill_bounds_are_safe_for_degenerate_inputs() {
        assert_eq!(fill_end(-0.5), 0);
        assert_eq!(fill_end(-10.5), -10);
        assert_eq!(fill_end(f32::NAN), 0);
        assert!(fill_end(f32::INFINITY) > 0);
        assert!(fill_end(f32::NEG_INFINITY) < 0);
        assert_eq!(fill_start(f32::NAN), 0);
        assert!(fill_start(f32::INFINITY) > 0);
        assert!(fill_start(f32::NEG_INFINITY) < 0);
    }
}
