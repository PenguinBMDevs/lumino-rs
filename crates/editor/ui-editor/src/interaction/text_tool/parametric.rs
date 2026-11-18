//! 参数化文字光栅化（DRAW-029 · Parametric，UI-023 接入屏幕比例）
//!
//! 与 `LegacyBox` 路径并行：本模块只读 [`TextParams`]，不改变旧路径的兼容行为。
//! 坐标单位是**采样格**，每格像素宽高由 [`RasterScale`] 给出；字号 / 字距 / 加粗
//! 一律按 `px_per_key`（buffer 像素 / 1 个 key）换算，不再假设方格。

use ab_glyph::{Font, Point as AbPoint, PxScale, ScaleFont};
use lumino_core::{TextAlign, TextParams, TextUnit};

use super::font::load_font_variant;
use super::rasterize::{InkRule, alpha_to_occupancy};
use super::scale::RasterScale;

/// 大写字母高度 / em 尺寸（同字体恒定；`Rows` 字号单位与 UI 单位换算共用）
///
/// 用 1000px 参考尺寸测 `H` 的墨迹高度，避免 1px 下的取整误差。
pub(super) fn cap_ratio(font: &ab_glyph::FontArc) -> f32 {
    const REFERENCE: f32 = 1000.0;
    font.outline_glyph(
        font.glyph_id('H')
            .with_scale_and_position(PxScale::from(REFERENCE), AbPoint::default()),
    )
    .map(|outline| {
        let b = outline.px_bounds();
        ((b.max.y - b.min.y).max(1e-3)) / REFERENCE
    })
    .unwrap_or(0.7)
}

/// 按字体族 + 字重 + 斜体取 cap height 比例（取不到字体时返回 `None`）
pub(super) fn cap_ratio_for(params: &TextParams) -> Option<f32> {
    load_font_variant(&params.font, params.weight, params.italic).map(|font| cap_ratio(&font))
}

/// 参数化文字灰度图（宽 = `cols × ss_x`，高 = `rows × ss_y`，行 0 = 顶部）
pub(crate) fn rasterize_glyph_alpha_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
    scale: &RasterScale,
) -> Option<(u32, u32, Vec<u8>)> {
    if text.is_empty() || cols == 0 || rows == 0 || params.validate().is_err() {
        return None;
    }
    let font = load_font_variant(&params.font, params.weight, params.italic)?;
    let w = scale.width(cols);
    let h = scale.height(rows);
    if w == 0 || h == 0 {
        return None;
    }

    // 1) 字号 → 像素：Font 单位直接按 keys；Rows 单位按大写 H 的高度换算。
    let px_per_key = scale.px_per_key;
    let px_size = match params.unit {
        TextUnit::Font => params.size * px_per_key,
        TextUnit::Rows => {
            // 与旧实现同一算式（参考尺寸 1000px 测 cap height），保证旧形状逐字节兼容
            let reference = 1000.0_f32;
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

/// 参数化占用网格：每个采样格按 threshold% 的 **key 轴覆盖率**判定
pub(crate) fn rasterize_text_params(
    text: &str,
    cols: usize,
    rows: usize,
    params: &TextParams,
    scale: &RasterScale,
) -> Option<Vec<Vec<bool>>> {
    let (_w, _h, buf) = rasterize_glyph_alpha_params(text, cols, rows, params, scale)?;
    let threshold = params.threshold.clamp(0.0, 100.0) / 100.0;
    let rule = if scale.legacy {
        // 旧 `TextOp` 的 Parametric 分支：面积占比、含等号（历史口径）
        InkRule::Area {
            threshold,
            inclusive: true,
        }
    } else {
        InkRule::KeyAxis { threshold }
    };
    Some(alpha_to_occupancy(&buf, cols, rows, scale, rule))
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

    /// 默认横向比例（snap=1920、ppq=480、k=0.4167 ⇒ 时间格 : key = 9.6 : 1）
    fn default_scale(cols: usize, rows: usize) -> RasterScale {
        RasterScale::from_k(false, 1920.0, 480.0, 20.0 / (480.0 * 0.1), cols, rows)
    }

    /// 墨迹 bbox（`(x0, y0, x1, y1)`，含端点）；无墨迹时返回 `None`
    fn ink_bbox(buf: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if buf[y * w + x] > 0 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        (x0 != usize::MAX).then_some((x0, y0, x1, y1))
    }

    #[test]
    fn test_threshold_moves_occupancy_boundary() {
        // 4×4 采样格、每格 4×4 子像素：首格填一半子像素行，第二格只填 1 个子像素
        let mut buf = vec![0u8; 16 * 16];
        for y in 0..2 {
            for x in 0..4 {
                buf[y * 16 + x] = 255;
            }
        }
        buf[4] = 255;
        let scale = RasterScale::legacy(false);
        let area = |threshold: f32| {
            alpha_to_occupancy(
                &buf,
                4,
                4,
                &scale,
                InkRule::Area {
                    threshold,
                    inclusive: true,
                },
            )
        };
        let low = area(0.25);
        let high = area(0.6);
        assert!(low[0][0], "25% 阈值应保留半个格");
        assert!(!high[0][0], "60% 阈值应剔除半个格");
        assert!(!low[1][0], "1/16 墨水不应通过 25% 阈值");

        // key 轴覆盖率：横向看 y，首格 2/4 行有墨 → 50%
        let key_low = alpha_to_occupancy(&buf, 4, 4, &scale, InkRule::KeyAxis { threshold: 0.5 });
        let key_high = alpha_to_occupancy(&buf, 4, 4, &scale, InkRule::KeyAxis { threshold: 0.6 });
        assert!(key_low[0][0], "50% key 轴覆盖率应保留两行墨");
        assert!(!key_high[0][0], "60% key 轴覆盖率应剔除两行墨");
    }

    /// key 轴覆盖率必须比面积覆盖率更「抗稀释」：
    /// 非正方形采样（38:4）下，一条细竖笔的面积占比约 1/38，但 key 轴覆盖率是 100%。
    #[test]
    fn test_key_axis_coverage_beats_area_on_thin_stroke() {
        let scale = default_scale(1, 4); // 横向：1 时间格 × 4 key，ss = 38.4 × 4
        let w = scale.width(1) as usize;
        let h = scale.height(4) as usize;
        assert_eq!((w, h), (38, 16), "默认比例下 1 时间格 = 38px、4 key = 16px");
        // 1px 竖笔：横向上只占 1/38，纵向上贯穿全部 4 个 key
        let mut buf = vec![0u8; w * h];
        for y in 0..h {
            buf[y * w + 18] = 255;
        }
        let area = alpha_to_occupancy(
            &buf,
            1,
            4,
            &scale,
            InkRule::Area {
                threshold: 0.5,
                inclusive: true,
            },
        );
        assert!(
            area.iter().flatten().all(|on| !on),
            "面积覆盖率下 1px 竖笔必然全丢（这正是 #273 的病灶）"
        );
        let key_axis = alpha_to_occupancy(&buf, 1, 4, &scale, InkRule::KeyAxis { threshold: 0.5 });
        let on = key_axis.iter().flatten().filter(|on| **on).count();
        assert_eq!(
            on, 4,
            "key 轴覆盖率下 4 个 key 都必须保留（第 {on} 个丢失）"
        );
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
        let scale = default_scale(24, 24);
        let Some((w, h, buf)) = rasterize_glyph_alpha_params("O", 24, 24, &params, &scale) else {
            return;
        };
        let (w, h) = (w as usize, h as usize);
        let Some((x0, y0, x1, y1)) = ink_bbox(&buf, w, h) else {
            return;
        };
        assert!(x1 > x0 && y1 > y0, "O 必须有墨迹");
        let center = ((y0 + y1) / 2) * w + (x0 + x1) / 2;
        assert_eq!(buf[center], 0, "O 的中心洞必须保持为空（nonzero）");
    }

    /// `TextUnit::Rows` 的语义是「大写高度占多少 key」：`H` 的墨迹高度应 ≈ 8 个 key。
    ///
    /// 横向卷帘下 key 轴是 Y，故量墨迹的 y 跨度；容差 10% 对应验收标准。
    #[test]
    fn test_rows_unit_scales_to_requested_cap_height() {
        let params = TextParams {
            unit: TextUnit::Rows,
            size: 8.0,
            threshold: 0.0,
            ..TextParams::default()
        };
        let scale = default_scale(24, 24);
        let Some((w, h, buf)) = rasterize_glyph_alpha_params("H", 24, 24, &params, &scale) else {
            return;
        };
        let (w, h) = (w as usize, h as usize);
        let Some((_, y0, _, y1)) = ink_bbox(&buf, w, h) else {
            return;
        };
        let ink_h = (y1 - y0 + 1) as f32;
        let expected = params.size * scale.px_per_key;
        assert!(
            (ink_h - expected).abs() / expected <= 0.1,
            "unit=Rows 时 H 的墨迹高度应约 {expected}px（8 key），实际 {ink_h}px"
        );
    }

    /// `cap_ratio` 是单位换算与 Rows 排版的共同基准，必须落在字体的合理区间
    #[test]
    fn test_cap_ratio_is_plausible() {
        let Some(font) = super::super::font::load_font("Microsoft YaHei") else {
            return;
        };
        let ratio = cap_ratio(&font);
        assert!(
            (0.5..1.2).contains(&ratio),
            "cap height / em 比例应在合理范围，实际 {ratio}"
        );
    }
}
