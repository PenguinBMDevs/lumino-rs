//! UI-023 CJK 回归：LegacyBox 的 letterbox 拟合高度必须用**墨迹高度**。
//!
//! Noto 的 `ascent + descent` 小于 CJK 墨迹高度；按行高定字号会把中文上下裁掉，
//! 表现为「字宽相对字高偏大」。这里用墨迹宽高比把这条量出来（误差 ≤5%）。

use ab_glyph::{Font, PxScale, ScaleFont};
use lumino_core::TextAlign;

use crate::interaction::text_tool::{RasterScale, rasterize_glyph_alpha_letterbox};

use super::bundled_font;

/// 墨迹 bbox：`(x0, y0, x1, y1)`，含端点；无墨迹时 clamp 成 0。
fn ink_bbox(buf: &[u8], w: usize, h: usize) -> (usize, usize, usize, usize) {
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
    assert!(x0 != usize::MAX, "应有墨迹");
    (x0, y0, x1, y1)
}

/// 同文本的墨迹宽高比：字形 ink bbox 并集（按 advance 排布），与字号无关。
///
/// 必须用**墨迹宽**而不是 advance 总宽：CJK 两侧留白会让 advance 比墨迹宽约 5%，
/// 用它当参考会把「正确实现」误判成超差。
fn natural_ink_aspect(font: &ab_glyph::FontArc, text: &str, px: f32) -> f32 {
    let scale = PxScale::from(px.max(1.0));
    let scaled = font.as_scaled(scale);
    let mut x_cursor = 0.0_f32;
    let (mut x0, mut x1) = (f32::MAX, f32::MIN);
    let (mut y0, mut y1) = (f32::MAX, f32::MIN);
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(outline) =
            font.outline_glyph(gid.with_scale_and_position(scale, ab_glyph::Point::default()))
        {
            let b = outline.px_bounds();
            x0 = x0.min(x_cursor + b.min.x);
            x1 = x1.max(x_cursor + b.max.x);
            y0 = y0.min(b.min.y);
            y1 = y1.max(b.max.y);
        }
        x_cursor += scaled.h_advance(gid);
    }
    (x1 - x0) / (y1 - y0).max(1e-3)
}

/// 默认视图：`k = 20 / (480 × 0.1)`，时间格 : key = 9.6 : 1。
fn default_k() -> f32 {
    20.0 / (480.0 * 0.1)
}

#[test]
fn test_letterbox_keeps_cjk_ink_aspect() {
    let font = bundled_font();
    let (cols, rows) = (24, 60);
    let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), cols, rows);
    let (w, h, buf) =
        rasterize_glyph_alpha_letterbox("中文", cols, rows, &font, &scale, TextAlign::Left)
            .expect("内置字体应能栅格化中文");

    let (x0, y0, x1, y1) = ink_bbox(&buf, w as usize, h as usize);
    let cell_ratio = 1920.0_f32 / (default_k() * 480.0);
    let measured =
        (((x1 - x0 + 1) as f32) / scale.ss_x) / (((y1 - y0 + 1) as f32) / scale.ss_y) * cell_ratio;
    let natural = natural_ink_aspect(&font, "中文", h as f32);
    assert!(
        (measured - natural).abs() / natural <= 0.05,
        "中文墨迹宽高比 {measured:.3} 与自然比 {natural:.3} 误差 >5%：letterbox 把 CJK 裁掉了"
    );
}
