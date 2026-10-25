//! 参数化文字光栅化（DRAW-029 · Parametric）
//!
//! 与 `LegacyBox` 路径并行：本模块只读 [`TextParams`]，不改变旧路径。坐标单位是
//! 文本框采样格（1 key = `SS` 个像素），因此预览与音符仍共用同一份栅格。

use ab_glyph::{Font, Point as AbPoint, PxScale, ScaleFont};
use lumino_core::{TextAlign, TextParams, TextUnit};

use super::font::load_font_variant;
use super::rasterize::SS;

/// 参数化文字灰度图（宽 = cols×SS，高 = rows×SS，行 0 = 顶部）
pub(crate) fn rasterize_glyph_alpha_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 || params.validate().is_err() {
        return None;
    }
    let font = load_font_variant(&params.font, params.weight, params.italic)?;
    let w = (cols as u32) * SS;
    let h = (rows as u32) * SS;

    // 1) 字号 → 像素：Font 单位直接按 keys；Rows 单位按大写 H 的高度换算。
    let px_per_key = SS as f32;
    let px_size = match params.unit {
        TextUnit::Font => params.size * px_per_key,
        TextUnit::Rows => {
            // 用 1000px 参考尺寸测 cap height，避免 1px 下的取整误差。
            let reference = 1000.0;
            let cap = font
                .outline_glyph(
                    font.glyph_id('H')
                        .with_scale_and_position(PxScale::from(reference), AbPoint::default()),
                )
                .map(|outline| {
                    let b = outline.px_bounds();
                    (b.max.y - b.min.y).max(1e-3)
                })
                .unwrap_or(reference * 0.7);
            params.size * px_per_key * reference / cap
        }
    };
    if !px_size.is_finite() || px_size <= 0.0 {
        return None;
    }

    let scale = PxScale::from(px_size);
    let scaled = font.as_scaled(scale);
    let line_height =
        ((scaled.ascent() - scaled.descent()) * (params.leading / 100.0).max(0.01)).max(1.0);
    let tracking_px = params.tracking * px_per_key;

    let lines: Vec<&str> = text.split('\n').collect();
    let widths: Vec<f32> = lines
        .iter()
        .map(|line| {
            let count = line.chars().count();
            let advance: f32 = line
                .chars()
                .map(|ch| scaled.h_advance(font.glyph_id(ch)))
                .sum();
            (advance + tracking_px * count.saturating_sub(1) as f32).max(0.0)
        })
        .collect();

    let mut buf = vec![0u8; (w as usize) * (h as usize)];
    // 最后一行底部对齐到框底；再逐行向上堆叠。
    let mut baseline = h as f32 + scaled.descent();
    for (line, width) in lines.iter().zip(widths.iter()).rev() {
        let x0 = match params.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (w as f32 - *width) * 0.5,
            TextAlign::Right => w as f32 - *width,
        }
        .max(0.0);
        let mut x_cursor = x0;
        for ch in line.chars() {
            let gid = font.glyph_id(ch);
            if let Some(outline) =
                font.outline_glyph(gid.with_scale_and_position(scale, AbPoint::default()))
            {
                let b = outline.px_bounds();
                outline.draw(|px, py, alpha| {
                    let x = (x_cursor + px as f32 + b.min.x).round() as i32;
                    let y = (baseline + py as f32 + b.min.y).round() as i32;
                    if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 && alpha > 0.0 {
                        let idx = y as usize * w as usize + x as usize;
                        buf[idx] = buf[idx].max((alpha * 255.0) as u8);
                    }
                });
            }
            x_cursor += scaled.h_advance(gid) + tracking_px;
        }
        baseline -= line_height;
    }

    if params.grow != 0.0 {
        let radius = (params.grow.abs() * px_per_key).round() as i32;
        // 防御异常输入：半径过大时退化为全图；UI 正常范围远小于此值。
        let radius = radius.clamp(0, 128);
        if radius > 0 {
            grow_alpha(&mut buf, w, h, radius, params.grow > 0.0);
        }
    }
    Some((w, h, buf))
}

/// 参数化占用网格：每个采样格按 threshold% 的子像素墨水判定
pub(crate) fn rasterize_text_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
) -> Option<Vec<Vec<bool>>> {
    let (w, _h, buf) = rasterize_glyph_alpha_params(text, cols, rows, params)?;
    Some(alpha_to_occupancy(&buf, w, cols, rows, params.threshold))
}

/// 灰度图 → 占用网格（纯函数，供阈值回归测试直接调用）
pub(super) fn alpha_to_occupancy(
    buf: &[u8],
    width: u32,
    cols: usize,
    rows: usize,
    threshold: f32,
) -> Vec<Vec<bool>> {
    let threshold = threshold.clamp(0.0, 100.0) / 100.0;
    let mut occ = vec![vec![false; cols]; rows];
    if cols == 0 || rows == 0 {
        return occ;
    }
    for (r, row) in occ.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let mut ink = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = c as u32 * SS + sx;
                    let y = r as u32 * SS + sy;
                    let idx = y as usize * width as usize + x as usize;
                    if buf.get(idx).copied().unwrap_or(0) > 0 {
                        ink += 1;
                    }
                }
            }
            *cell = ink > 0 && (ink as f32 / (SS * SS) as f32) >= threshold;
        }
    }
    occ
}

/// 对灰度图做方形核膨胀/腐蚀（`grow > 0` 变粗，`< 0` 变细）
fn grow_alpha(buf: &mut [u8], w: u32, h: u32, radius: i32, dilate: bool) {
    let src = buf.to_vec();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut value = if dilate { 0u8 } else { 255u8 };
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let sx = x + dx;
                    let sy = y + dy;
                    if sx < 0 || sy < 0 || sx >= w as i32 || sy >= h as i32 {
                        continue;
                    }
                    let sample = src[sy as usize * w as usize + sx as usize];
                    value = if dilate {
                        value.max(sample)
                    } else {
                        value.min(sample)
                    };
                }
            }
            buf[y as usize * w as usize + x as usize] = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_threshold_moves_occupancy_boundary() {
        let mut buf = vec![0u8; 16 * 16];
        // 第一个 4x4 格填一半，第二个只填 1/16
        for y in 0..2 {
            for x in 0..4 {
                buf[y * 16 + x] = 255;
            }
        }
        buf[4] = 255;
        let low = alpha_to_occupancy(&buf, 16, 4, 4, 25.0);
        let high = alpha_to_occupancy(&buf, 16, 4, 4, 60.0);
        assert!(low[0][0], "25% 阈值应保留半个格");
        assert!(!high[0][0], "60% 阈值应剔除半个格");
        assert!(!low[1][0], "1/16 墨水不应通过 25% 阈值");
    }

    #[test]
    fn test_grow_dilates_and_erodes() {
        let mut buf = vec![0u8; 9 * 9];
        buf[4 * 9 + 4] = 255;
        let mut dilated = buf.clone();
        grow_alpha(&mut dilated, 9, 9, 1, true);
        assert!(dilated[3 * 9 + 4] > 0, "膨胀应向外扩一格");

        let mut solid = vec![0u8; 9 * 9];
        for y in 2..7 {
            for x in 2..7 {
                solid[y * 9 + x] = 255;
            }
        }
        grow_alpha(&mut solid, 9, 9, 1, false);
        assert_eq!(solid[2 * 9 + 2], 0, "腐蚀应削掉实心块边界");
        assert_eq!(solid[4 * 9 + 4], 255, "腐蚀应保留实心块内部");
    }

    #[test]
    fn test_parametric_render_keeps_glyph_hole() {
        let params = TextParams {
            size: 20.0,
            threshold: 10.0,
            ..TextParams::default()
        };
        let Some((_w, _h, buf)) = rasterize_glyph_alpha_params("O", 24, 24, &params) else {
            return;
        };
        let w = 24 * SS;
        let center = (12 * SS) as usize * w as usize + (12 * SS) as usize;
        assert_eq!(buf[center], 0, "O 的中心洞必须保持为空（nonzero）");
    }

    #[test]
    fn test_rows_unit_scales_to_requested_cap_height() {
        let params = TextParams {
            unit: TextUnit::Rows,
            size: 8.0,
            threshold: 0.0,
            ..TextParams::default()
        };
        let Some(occ) = rasterize_text_params("H", 24, 24, &params) else {
            return;
        };
        let rows_on = occ.iter().filter(|row| row.iter().any(|&on| on)).count();
        assert!(
            (7..=9).contains(&rows_on),
            "unit=Rows should make H about 8 keys tall, got {rows_on}"
        );
    }
}
