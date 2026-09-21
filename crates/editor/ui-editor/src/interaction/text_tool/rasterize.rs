use ab_glyph::{Font, Point as AbPoint, PxScale, ScaleFont};
use iced_core::Point;

use super::font::load_font;

/// 字形光栅化超采样倍数（每个 key 列 / key 行细分为 SS×SS 子像素，提升细笔画捕获）
const SS: u32 = 4;
/// 单元格占用判定阈值：墨水子像素占比高于此值视为「有墨水」
const COVERAGE_THRESHOLD: f32 = 0.08;

/// 点是否落在矩形内
pub(super) fn point_in_rect(p: Point, r: iced_core::Rectangle) -> bool {
    p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height
}

/// 将文字光栅化为灰度位图（行优先，**行 0 = 文字顶部**，底部对齐到框底）。
///
/// 返回 `(width, height, buf)`：`width = cols * SS`、`height = rows * SS`，`buf` 按行主序存储，
/// 每像素为 0..255 的墨水墨度。预览渲染与音符采样共用同一份栅格，保证「看到的就是生成的」。
pub(crate) fn rasterize_glyph_alpha(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 {
        return None;
    }
    let font = load_font(family)?;
    let h = (rows as u32) * SS;
    let w = (cols as u32) * SS;

    // 垂直缩放：用字体 ascent+descent（不含行距）充满框高，使文字填满框且不留顶部空白。
    let unit = font.as_scaled(PxScale::from(1.0));
    let h1 = (unit.ascent() + unit.descent()).max(1e-3);
    let scale = PxScale::from(h as f32 / h1);
    let scaled = font.as_scaled(scale);

    // 第一遍：计算自然布局总推进宽度，并记录所有字形中最低墨水的 y（渲染像素，y 向下）。
    let mut total_advance = 0f32;
    let mut max_bottom = 0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        total_advance += scaled.h_advance(gid);
        if let Some(outline) =
            font.outline_glyph(gid.with_scale_and_position(scale, AbPoint::default()))
        {
            let b = outline.px_bounds();
            if b.max.y > max_bottom {
                max_bottom = b.max.y;
            }
        }
    }
    let tw = total_advance.max(1.0).ceil() as u32;

    // 底部对齐：把最低墨水（max_bottom）对齐到缓冲底（行 h）。
    let y0 = (h as f32) - max_bottom;

    // 渲染到临时缓冲（高 = h，宽 = 自然推进）
    let mut temp = vec![0u8; (h as usize) * (tw as usize)];
    let mut x_cursor = 0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, AbPoint::default());
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|px, py, alpha| {
                // px/py 为相对字形包围盒左上角的渲染像素坐标（x 向右、y 向下，无翻转）。
                // px_bounds().min 为字形在基线坐标系下的原点偏移；加 y0 实现底部对齐。
                let x = (x_cursor + px as f32 + b.min.x).round() as i32;
                let y = (py as f32 + b.min.y + y0).round() as i32;
                if x >= 0 && x < tw as i32 && y >= 0 && y < h as i32 && alpha > 0.0 {
                    temp[y as usize * tw as usize + x as usize] = (alpha * 255.0) as u8;
                }
            });
        }
        x_cursor += scaled.h_advance(gid);
    }

    // 水平拉伸到 w（高度已一致），得到铺满框的位图
    let mut buf = vec![0u8; (h as usize) * (w as usize)];
    for (r, buf_row) in buf.chunks_mut(w as usize).enumerate() {
        let temp_base = r * tw as usize;
        for (c, buf_cell) in buf_row.iter_mut().enumerate() {
            let src_x = if tw == 0 {
                0
            } else {
                ((c as f64 * tw as f64 / w as f64) as u32).min(tw - 1)
            };
            *buf_cell = temp[temp_base + src_x as usize];
        }
    }
    Some((w, h, buf))
}

/// 将文字光栅化为占用网格（rows × cols，[row][col] = 是否有墨水）
///
/// 行 0 = 文字顶部。文字**底部对齐**到框底（共用基线），并按框高度填满、按框宽度拉伸。
/// 返回 `None` 表示无字体或文字为空。
pub(crate) fn rasterize_text(
    text: &str,
    cols: usize,
    rows: usize,
    family: &str,
) -> Option<Vec<Vec<bool>>> {
    let (w, _h, buf) = rasterize_glyph_alpha(text, cols, rows, family)?;

    // 由 SS×SS 子像素区域判定每个 (col,row) 是否占用
    let mut occ = vec![vec![false; cols]; rows];
    for (r, row) in occ.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let mut ink = 0u32;
            let mut total = 0u32;
            for sr in (r as u32 * SS)..((r as u32 + 1) * SS) {
                for sc in (c as u32 * SS)..((c as u32 + 1) * SS) {
                    total += 1;
                    if buf[sr as usize * w as usize + sc as usize] > 0 {
                        ink += 1;
                    }
                }
            }
            *cell = (ink as f32 / total as f32) > COVERAGE_THRESHOLD;
        }
    }
    Some(occ)
}

/// 纯函数：将占用网格转换为音符列表（不依赖字体 / 画布）。
///
/// `merged = false`（正常采样）：每个有墨水的 (col,row) 生成一个音符，长度 = snap；
/// `merged = true`（key 范围合并）：每个 key 行内连续有墨水的列合并为一个音符，
/// 任意空隙断开；音符长度 = 连续列数 × snap。
///
/// `key_top` 为文字顶部对应的 key（行 0 映射到 `key_top`，向下递减）。
pub(crate) fn sample_to_notes(
    occupancy: &[Vec<bool>],
    tick_lo: f32,
    key_top: i32,
    snap: f32,
    merged: bool,
) -> Vec<(f32, u16, f32)> {
    let mut notes = Vec::new();
    if merged {
        for (r, row) in occupancy.iter().enumerate() {
            let mut run_start: Option<usize> = None;
            for (c, &cell) in row.iter().enumerate() {
                match (run_start, cell) {
                    (Some(_), true) => continue,
                    (Some(start), false) => {
                        let end = c.saturating_sub(1);
                        if end >= start {
                            let key = (key_top - r as i32).clamp(0, 255) as u16;
                            let tick = tick_lo + start as f32 * snap;
                            let len = (end - start + 1) as f32 * snap;
                            notes.push((tick, key, len));
                        }
                        run_start = None;
                    }
                    (None, true) => run_start = Some(c),
                    (None, false) => {}
                }
            }
            // 行尾收尾：仍有一段未闭合的连续墨水
            if let Some(start) = run_start {
                let end = row.len().saturating_sub(1);
                if end >= start {
                    let key = (key_top - r as i32).clamp(0, 255) as u16;
                    let tick = tick_lo + start as f32 * snap;
                    let len = (end - start + 1) as f32 * snap;
                    notes.push((tick, key, len));
                }
            }
        }
    } else {
        for (r, row) in occupancy.iter().enumerate() {
            for (c, &cell) in row.iter().enumerate() {
                if cell {
                    let key = (key_top - r as i32).clamp(0, 255) as u16;
                    let tick = tick_lo + c as f32 * snap;
                    notes.push((tick, key, snap));
                }
            }
        }
    }
    notes
}
