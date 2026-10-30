//! 单音符矩形光栅化（全质量填充 + 2px 描边）。

use lumino_midi_loader::NoteEvent;

use crate::render::{
    band::{BandJob, BandStats},
    buffer::BandBuffer,
    color::NOTE_BORDER_PX,
    simd::{SimdLevel, masked_fill_row},
};

pub(super) fn draw_note(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    is_main: bool,
    note: &NoteEvent,
    stats: &mut BandStats,
) {
    let camera = &job.camera;
    let zoom_x = camera.zoom_x;
    let sx = note.start_tick as f32 * zoom_x - camera.scroll_x + camera.keyboard_width;
    let sw = note.length() as f32 * zoom_x;
    if sw <= 0.0 {
        return;
    }
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    if sh <= 0.0 {
        return;
    }

    // 像素中心覆盖规则：sx <= i+0.5 < sx+sw。
    let mut ix0 = (sx - 0.5).ceil() as i32;
    let mut ix1 = (sx + sw - 0.5).ceil() as i32;
    let mut iy0 = (sy - 0.5).ceil() as i32;
    let mut iy1 = (sy + sh - 0.5).ceil() as i32;
    ix0 = ix0.max(job.x0 as i32);
    ix1 = ix1.min(job.x1 as i32);
    iy0 = iy0.max(0);
    iy1 = iy1.min(camera.height as i32);
    if ix1 <= ix0 || iy1 <= iy0 {
        return;
    }

    let color = if is_main {
        job.style.fill(track, true)
    } else {
        job.style.fill(track, false)
    };
    let border_color = job.style.border(track, is_main);
    let (owner_value, owner_limit) = if is_main { (2, 1) } else { (1, 0) };

    // 每侧实际边框 = 2 × border_width = 2px。填充分界（浮点）。
    let border_px = NOTE_BORDER_PX;
    let fx0_f = sx + border_px;
    let fx1_f = sx + sw - border_px;
    let fy0_f = sy + border_px;
    let fy1_f = sy + sh - border_px;
    let mut fx0 = (fx0_f - 0.5).ceil() as i32;
    let mut fx1 = (fx1_f - 0.5).ceil() as i32;
    let mut fy0 = (fy0_f - 0.5).ceil() as i32;
    let mut fy1 = (fy1_f - 0.5).ceil() as i32;
    fx0 = fx0.max(ix0);
    fx1 = fx1.min(ix1);
    fy0 = fy0.max(iy0);
    fy1 = fy1.min(iy1);
    let has_fill = fx1 > fx0 && fy1 > fy0;

    if !has_fill {
        // 细音符/纯描边：宽度 ≤ 8px 时直接遍历像素，避免每行一次函数调用。
        let width_px = ix1 - ix0;
        if width_px == 1 {
            draw_pixel_column(
                buffer,
                ix0 - job.x0 as i32,
                iy0,
                iy1,
                border_color,
                owner_value,
                owner_limit,
            );
        } else if width_px <= 8 {
            draw_owner_block(
                buffer,
                job.x0,
                ix0,
                ix1,
                iy0,
                iy1,
                border_color,
                owner_value,
                owner_limit,
            );
        } else {
            for y in iy0..iy1 {
                draw_segment(
                    buffer,
                    y,
                    job.x0,
                    ix0,
                    ix1,
                    border_color,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
        }
        stats.visible_notes += 1;
        return;
    }

    for y in iy0..iy1 {
        if y >= fy0 && y < fy1 {
            if fx0 > ix0 {
                draw_segment(
                    buffer,
                    y,
                    job.x0,
                    ix0,
                    fx0,
                    border_color,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
            draw_segment(
                buffer,
                y,
                job.x0,
                fx0,
                fx1,
                color,
                owner_value,
                owner_limit,
                job.simd,
            );
            if ix1 > fx1 {
                draw_segment(
                    buffer,
                    y,
                    job.x0,
                    fx1,
                    ix1,
                    border_color,
                    owner_value,
                    owner_limit,
                    job.simd,
                );
            }
        } else {
            draw_segment(
                buffer,
                y,
                job.x0,
                ix0,
                ix1,
                border_color,
                owner_value,
                owner_limit,
                job.simd,
            );
        }
    }
    stats.visible_notes += 1;
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
) {
    let lx0 = (x0 - band_x0 as i32).max(0) as usize;
    let lx1 = (x1 - band_x0 as i32).min(buffer.width as i32).max(0) as usize;
    if lx1 <= lx0 {
        return;
    }
    let width = buffer.width;
    let start_y = y0.max(0) as usize;
    let end_y = (y1.min(buffer.height as i32)).max(0) as usize;
    for y in start_y..end_y {
        let base = y * width;
        for x in lx0..lx1 {
            let index = base + x;
            if buffer.owner[index] <= owner_limit {
                buffer.pixels[index] = color;
                buffer.owner[index] = owner_value;
            }
        }
    }
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
) {
    let lx0 = x0 - band_x0 as i32;
    let lx1 = x1 - band_x0 as i32;
    if lx1 <= lx0 {
        return;
    }
    let width = buffer.width;
    let row_start = y as usize * width;
    let (pixels, owner) = (&mut buffer.pixels, &mut buffer.owner);
    let row = &mut pixels[row_start..row_start + width];
    let own = &mut owner[row_start..row_start + width];
    masked_fill_row(row, own, lx0, lx1, color, owner_value, owner_limit, simd);
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
) {
    if x < 0 || x >= buffer.width as i32 {
        return;
    }
    let x = x as usize;
    for y in y0..y1 {
        let index = y as usize * buffer.width + x;
        if buffer.owner[index] <= owner_limit {
            buffer.pixels[index] = color;
            buffer.owner[index] = owner_value;
        }
    }
}
