//! 参数化文字光栅化（DRAW-029 · Parametric）
//!
//! 与 `LegacyBox` 路径并行：本模块只读 [`TextParams`]。坐标单位是文本框采样格，
//! 但 buffer 像素在屏幕上是各向同性的（见 [`RasterScale`]），字号 / 字距 / grow
//! 统一按 `px_per_key` 换算。

use ab_glyph::{Font, FontArc, Point as AbPoint, PxScale, ScaleFont};
use lumino_core::{TextAlign, TextParams, TextUnit};

use super::font::load_font_variant;
use super::occupancy::alpha_to_occupancy;
use super::scale::{HARD_MAX_PIXELS, RasterScale};

/// 参数化文字灰度图（宽 = cols×ss_x，高 = rows×ss_y，行 0 = 顶部）
pub(crate) fn rasterize_glyph_alpha_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
    scale: RasterScale,
) -> Option<(u32, u32, Vec<u8>)> {
    let font = load_font_variant(&params.font, params.weight, params.italic)?;
    rasterize_glyph_alpha_params_with_font(text, cols, rows, params, &font, scale)
}

/// 与 [`rasterize_glyph_alpha_params`] 同一实现，但字体由调用方注入（测试用）。
pub(super) fn rasterize_glyph_alpha_params_with_font(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
    font: &FontArc,
    scale: RasterScale,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 || params.validate().is_err() {
        return None;
    }
    let (w, h) = scale.buffer_dims(cols, rows);
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > HARD_MAX_PIXELS {
        return None;
    }
    let px_per_key = scale.px_per_key.max(1e-3);

    // 1) 字号 → 像素：Font 单位直接按 keys；Rows 单位按大写 H 的高度换算。
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

    let scale_px = PxScale::from(px_size);
    let scaled = font.as_scaled(scale_px);
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
                font.outline_glyph(gid.with_scale_and_position(scale_px, AbPoint::default()))
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

/// 参数化占用网格：每格按 pitch 轴覆盖率 / threshold 判定
pub(crate) fn rasterize_text_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
    scale: RasterScale,
) -> Option<Vec<Vec<bool>>> {
    let (w, _h, buf) = rasterize_glyph_alpha_params(text, cols, rows, params, scale)?;
    Some(alpha_to_occupancy(
        &buf,
        w,
        cols,
        rows,
        scale,
        params.threshold,
    ))
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
        let scale = RasterScale::legacy(false);
        let low = alpha_to_occupancy(&buf, 16, 4, 4, scale, 25.0);
        let high = alpha_to_occupancy(&buf, 16, 4, 4, scale, 60.0);
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
        let Some((_w, _h, buf)) =
            rasterize_glyph_alpha_params("O", 24, 24, &params, RasterScale::legacy(false))
        else {
            return;
        };
        let scale = RasterScale::legacy(false);
        let w = 24 * scale.ss_x as usize;
        let center = (12 * scale.ss_y) as usize * w + (12 * scale.ss_x) as usize;
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
        let Some(occ) = rasterize_text_params("H", 24, 24, &params, RasterScale::legacy(false))
        else {
            return;
        };
        let rows_on = occ.iter().filter(|row| row.iter().any(|&on| on)).count();
        assert!(
            (7..=9).contains(&rows_on),
            "unit=Rows should make H about 8 keys tall, got {rows_on}"
        );
    }
}
