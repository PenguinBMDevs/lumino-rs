//! 单音符矩形光栅化（全质量填充 + 2px 描边）。

use lumino_midi_loader::NoteEvent;

use crate::render::{
    band::{BandJob, BandStats},
    buffer::BandBuffer,
    color::NOTE_BORDER_PX,
    simd::{SimdLevel, masked_fill_row},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_note(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    is_main: bool,
    note: &NoteEvent,
    stats: &mut BandStats,
    count_coverage: bool,
    ruler_rows: usize,
) -> usize {
    let camera = &job.camera;
    let zoom_x = camera.zoom_x;
    let sx = note.start_tick as f32 * zoom_x - camera.scroll_x + camera.keyboard_width;
    let sw = note.length() as f32 * zoom_x;
    if sw <= 0.0 {
        return 0;
    }
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    if sh <= 0.0 {
        return 0;
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
        return 0;
    }

    let pass = note_pass(job, track, is_main, count_coverage, ruler_rows);

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

    let rects = NoteRects {
        ix0,
        ix1,
        iy0,
        iy1,
        fx0,
        fx1,
        fy0,
        fy1,
    };

    let mut covered = 0usize;

    if !has_fill {
        covered = draw_outline_pass(job, buffer, &rects, &pass, covered);
        stats.visible_notes += 1;
        return covered;
    }

    covered = draw_fill_pass(job, buffer, &rects, &pass, covered);
    stats.visible_notes += 1;
    covered
}

/// 单音符解析出的屏幕像素边界（`draw_note` 的几何部分）。
struct NoteRects {
    /// x 方向外框区间 `[ix0, ix1)`。
    ix0: i32,
    ix1: i32,
    /// y 方向外框区间 `[iy0, iy1)`。
    iy0: i32,
    iy1: i32,
    /// 填充区间 `[fx0, fx1) × [fy0, fy1)`。
    fx0: i32,
    fx1: i32,
    fy0: i32,
    fy1: i32,
}

/// 本次 pass 的配色、owner 归属与覆盖统计开关。
struct NotePass {
    /// 填充色。
    color: u32,
    /// 描边色。
    border_color: u32,
    /// 本次 pass 写入的 owner 值（洋葱皮 1、主轨 2）。
    owner_value: u32,
    /// 允许被覆盖的 owner 上限。
    owner_limit: u32,
    /// 是否统计新覆盖像素（仅洋葱皮 pass）。
    count_coverage: bool,
    /// 标尺行数：其上才计入覆盖。
    ruler_rows: usize,
}

/// 解析本次 pass 的配色、owner 归属与覆盖统计开关。
fn note_pass(
    job: &BandJob,
    track: usize,
    is_main: bool,
    count_coverage: bool,
    ruler_rows: usize,
) -> NotePass {
    let color = if is_main {
        job.style.fill(track, true)
    } else {
        job.style.fill(track, false)
    };
    let border_color = job.style.border(track, is_main);
    let (owner_value, owner_limit) = if is_main { (2, 1) } else { (1, 0) };

    NotePass {
        color,
        border_color,
        owner_value,
        owner_limit,
        count_coverage,
        ruler_rows,
    }
}

/// 细音符/纯描边 pass：宽度 ≤8px 时逐像素写，更宽时逐行覆盖。
fn draw_outline_pass(
    job: &BandJob,
    buffer: &mut BandBuffer,
    rects: &NoteRects,
    pass: &NotePass,
    mut covered: usize,
) -> usize {
    // 细音符/纯描边：宽度 ≤ 8px 时直接遍历像素，避免每行一次函数调用。
    let width_px = rects.ix1 - rects.ix0;
    if width_px == 1 {
        covered += draw_pixel_column(
            buffer,
            rects.ix0 - job.x0 as i32,
            rects.iy0,
            rects.iy1,
            pass.border_color,
            pass.owner_value,
            pass.owner_limit,
            pass.count_coverage,
            pass.ruler_rows,
        );
    } else if width_px <= 8 {
        covered += draw_owner_block(
            buffer,
            job.x0,
            rects.ix0,
            rects.ix1,
            rects.iy0,
            rects.iy1,
            pass.border_color,
            pass.owner_value,
            pass.owner_limit,
            pass.count_coverage,
            pass.ruler_rows,
        );
    } else {
        for y in rects.iy0..rects.iy1 {
            covered += draw_segment(
                buffer,
                y,
                job.x0,
                rects.ix0,
                rects.ix1,
                pass.border_color,
                pass.owner_value,
                pass.owner_limit,
                job.simd,
                pass.count_coverage,
                pass.ruler_rows,
            );
        }
    }
    covered
}

/// 有填充区 pass：逐行画左右描边与中间填充。
fn draw_fill_pass(
    job: &BandJob,
    buffer: &mut BandBuffer,
    rects: &NoteRects,
    pass: &NotePass,
    mut covered: usize,
) -> usize {
    for y in rects.iy0..rects.iy1 {
        if y >= rects.fy0 && y < rects.fy1 {
            if rects.fx0 > rects.ix0 {
                covered += draw_segment(
                    buffer,
                    y,
                    job.x0,
                    rects.ix0,
                    rects.fx0,
                    pass.border_color,
                    pass.owner_value,
                    pass.owner_limit,
                    job.simd,
                    pass.count_coverage,
                    pass.ruler_rows,
                );
            }
            covered += draw_segment(
                buffer,
                y,
                job.x0,
                rects.fx0,
                rects.fx1,
                pass.color,
                pass.owner_value,
                pass.owner_limit,
                job.simd,
                pass.count_coverage,
                pass.ruler_rows,
            );
            if rects.ix1 > rects.fx1 {
                covered += draw_segment(
                    buffer,
                    y,
                    job.x0,
                    rects.fx1,
                    rects.ix1,
                    pass.border_color,
                    pass.owner_value,
                    pass.owner_limit,
                    job.simd,
                    pass.count_coverage,
                    pass.ruler_rows,
                );
            }
        } else {
            covered += draw_segment(
                buffer,
                y,
                job.x0,
                rects.ix0,
                rects.ix1,
                pass.border_color,
                pass.owner_value,
                pass.owner_limit,
                job.simd,
                pass.count_coverage,
                pass.ruler_rows,
            );
        }
    }
    covered
}

/// 列主序绘制：窄音符的 20 个纵向像素连续写，避免行主序的 cache line 抖动。
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_note_column(
    job: &BandJob,
    buffer: &mut BandBuffer,
    track: usize,
    is_main: bool,
    note: &NoteEvent,
    stats: &mut BandStats,
    count_coverage: bool,
    ruler_rows: usize,
) -> usize {
    let camera = &job.camera;
    let sx = note.start_tick as f32 * camera.zoom_x - camera.scroll_x + camera.keyboard_width;
    let sw = note.length() as f32 * camera.zoom_x;
    if sw <= 0.0 {
        return 0;
    }
    let sy = camera.key_to_y(note.key as u16);
    let sh = camera.zoom_y;
    if sh <= 0.0 {
        return 0;
    }
    let mut ix0 = (sx - 0.5).ceil() as i32;
    let mut ix1 = (sx + sw - 0.5).ceil() as i32;
    let mut iy0 = (sy - 0.5).ceil() as i32;
    let mut iy1 = (sy + sh - 0.5).ceil() as i32;
    ix0 = ix0.max(job.x0 as i32);
    ix1 = ix1.min(job.x1 as i32);
    iy0 = iy0.max(0);
    iy1 = iy1.min(camera.height as i32);
    if ix1 <= ix0 || iy1 <= iy0 {
        return 0;
    }
    let color = if is_main {
        job.style.fill(track, true)
    } else {
        job.style.fill(track, false)
    };
    let border_color = job.style.border(track, is_main);
    let (owner_value, owner_limit) = if is_main { (2, 1) } else { (1, 0) };
    let border_px = NOTE_BORDER_PX;
    let fx0 = ((sx + border_px - 0.5).ceil() as i32).max(ix0);
    let fx1 = ((sx + sw - border_px - 0.5).ceil() as i32).min(ix1);
    let fy0 = ((sy + border_px - 0.5).ceil() as i32).max(iy0);
    let fy1 = ((sy + sh - border_px - 0.5).ceil() as i32).min(iy1);
    let has_fill = fx1 > fx0 && fy1 > fy0;
    let height = buffer.height;
    let mut covered = 0usize;
    for x in ix0..ix1 {
        let local_x = x - job.x0 as i32;
        if local_x < 0 || local_x as usize >= buffer.width {
            continue;
        }
        let base = local_x as usize * height;
        for y in iy0..iy1 {
            let pixel_color = if has_fill && y >= fy0 && y < fy1 && x >= fx0 && x < fx1 {
                color
            } else {
                border_color
            };
            let index = base + y as usize;
            if buffer.column_owner[index] <= owner_limit {
                if count_coverage && (y as usize) >= ruler_rows && buffer.column_owner[index] == 0 {
                    covered += 1;
                }
                buffer.column_pixels[index] = pixel_color;
                buffer.column_owner[index] = owner_value;
            }
        }
    }
    stats.visible_notes += 1;
    covered
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
    count_coverage: bool,
    ruler_rows: usize,
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
        let count_row = count_coverage && y >= ruler_rows;
        for x in lx0..lx1 {
            let index = base + x;
            if buffer.owner[index] <= owner_limit {
                if count_row && buffer.owner[index] == 0 {
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
    count_coverage: bool,
    ruler_rows: usize,
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
    masked_fill_row(
        row,
        own,
        lx0,
        lx1,
        color,
        owner_value,
        owner_limit,
        simd,
        count_coverage && (y as usize) >= ruler_rows,
    )
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
    count_coverage: bool,
    ruler_rows: usize,
) -> usize {
    if x < 0 || x >= buffer.width as i32 {
        return 0;
    }
    let x = x as usize;
    let mut covered = 0usize;
    for y in y0..y1 {
        let index = y as usize * buffer.width + x;
        if buffer.owner[index] <= owner_limit {
            if count_coverage && (y as usize) >= ruler_rows && buffer.owner[index] == 0 {
                covered += 1;
            }
            buffer.pixels[index] = color;
            buffer.owner[index] = owner_value;
        }
    }
    covered
}
