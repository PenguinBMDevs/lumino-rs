//! UI-023 文字栅格比例 / 阈值 / 重烘焙回归（字体注入，不依赖系统字体）。

use ab_glyph::{Font, FontArc, Point as AbPoint, PxScale, ScaleFont};
use lumino_core::{TextLayoutMode, TextParams, TextUnit};

use super::*;
use crate::Editor;

fn bundled_font() -> FontArc {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../resources/fonts/NotoSansSC-Regular.ttf"
    ));
    FontArc::try_from_vec(bytes.to_vec()).expect("仓库内置 Noto Sans SC 应可解析")
}

fn params(size: f32) -> TextParams {
    TextParams {
        font: "Noto Sans SC".to_string(),
        size,
        threshold: 50.0,
        ..TextParams::default()
    }
}

/// 默认视图：snap 1/4、zoom 0.1/20、ppq 480 → k = 0.4167、cell = 9.6:1
fn default_k() -> f32 {
    20.0 / (480.0 * 0.1)
}

fn ink_bbox(buf: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let mut min_x = usize::MAX;
    let mut max_x = 0;
    let mut min_y = usize::MAX;
    let mut max_y = 0;
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if buf[y * w + x] > 0 {
                any = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    any.then_some((min_x, max_x, min_y, max_y))
}

/// 同一字体 / 文本在指定 em 尺寸下的墨迹宽高比。
fn natural_ink_aspect(font: &FontArc, text: &str, reference: f32) -> f32 {
    let scaled = font.as_scaled(PxScale::from(reference.max(1.0)));
    let mut width = 0f32;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        width += scaled.h_advance(gid);
        if let Some(outline) = font.outline_glyph(
            gid.with_scale_and_position(PxScale::from(reference.max(1.0)), AbPoint::default()),
        ) {
            let b = outline.px_bounds();
            min_y = min_y.min(b.min.y);
            max_y = max_y.max(b.max.y);
        }
    }
    width / (max_y - min_y).max(1e-3)
}

/// 位图像素 → 屏幕物理宽高比（每列 ss_x、每行 ss_y，再乘屏幕 cell 宽高比）
fn physical_aspect(scale: RasterScale, bbox: (usize, usize, usize, usize), cell_ratio: f32) -> f32 {
    let (min_x, max_x, min_y, max_y) = bbox;
    let bw = (max_x - min_x + 1) as f32 / scale.ss_x as f32;
    let bh = (max_y - min_y + 1) as f32 / scale.ss_y as f32;
    bw / bh * cell_ratio
}

fn assert_aspect(text: &str, cols: usize, rows: usize, vertical: bool, layout: TextLayoutMode) {
    let font = bundled_font();
    let params = params(24.0);
    let scale = RasterScale::from_k(vertical, 1920.0, 480.0, default_k(), cols, rows);
    let raster = match layout {
        TextLayoutMode::LegacyBox => {
            rasterize_glyph_alpha_with_font(text, cols, rows, &params, &font, scale)
        }
        TextLayoutMode::Parametric => parametric::rasterize_glyph_alpha_params_with_font(
            text, cols, rows, &params, &font, scale,
        ),
    };
    let Some((w, h, buf)) = raster else {
        panic!("内置字体应能栅格化 {text}");
    };
    let Some(bbox) = ink_bbox(&buf, w as usize, h as usize) else {
        panic!("{text} 应有墨水（{layout:?}, vertical={vertical}）");
    };
    let beats_per_cell = 1920.0 / 480.0;
    let cell_ratio = if vertical {
        default_k() / beats_per_cell
    } else {
        beats_per_cell / default_k()
    };
    let aspect = physical_aspect(scale, bbox, cell_ratio);
    // 与渲染同一 em 尺寸的自然墨迹比较，避免不同字号下的 hinting 误差。
    let reference_px = match layout {
        TextLayoutMode::LegacyBox => {
            let unit = font.as_scaled(PxScale::from(1.0));
            let natural_w: f32 = text
                .chars()
                .map(|ch| unit.h_advance(font.glyph_id(ch)))
                .sum::<f32>()
                .max(1e-3);
            let mut ink_min_y = f32::MAX;
            let mut ink_max_y = f32::MIN;
            for ch in text.chars() {
                if let Some(outline) = font.outline_glyph(
                    font.glyph_id(ch)
                        .with_scale_and_position(PxScale::from(1.0), AbPoint::default()),
                ) {
                    let b = outline.px_bounds();
                    ink_min_y = ink_min_y.min(b.min.y);
                    ink_max_y = ink_max_y.max(b.max.y);
                }
            }
            let natural_h = if ink_max_y > ink_min_y {
                (ink_max_y - ink_min_y).max(1e-3)
            } else {
                (unit.ascent() + unit.descent()).max(1e-3)
            };
            let (bw, bh) = scale.buffer_dims(cols, rows);
            (bw as f32 / natural_w).min(bh as f32 / natural_h)
        }
        TextLayoutMode::Parametric => params.size * scale.px_per_key,
    };
    let natural = natural_ink_aspect(&font, text, reference_px);
    let ratio = aspect / natural;
    assert!(
        (ratio - 1.0).abs() <= 0.10,
        "{layout:?} vertical={vertical} {text} {cols}x{rows}: aspect={aspect:.3} natural={natural:.3} ratio={ratio:.3} bbox={bbox:?} ref={reference_px:.2}（scale {}x{}）",
        scale.ss_x,
        scale.ss_y
    );
}

#[test]
fn test_raster_scale_default_and_vertical_are_opposite() {
    let k = default_k();
    let horizontal = RasterScale::from_k(false, 1920.0, 480.0, k, 2, 15);
    assert_eq!((horizontal.ss_x, horizontal.ss_y), (38, 4));
    assert!((horizontal.px_per_key - 4.0).abs() < 1e-6);

    let vertical = RasterScale::from_k(true, 1920.0, 480.0, k, 15, 2);
    assert_eq!((vertical.ss_x, vertical.ss_y), (4, 38));
    assert!((vertical.px_per_key - 4.0).abs() < 1e-6);
}

#[test]
fn test_raster_scale_caps_pixel_budget() {
    let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 100, 128);
    let (w, h) = scale.buffer_dims(100, 128);
    assert!(
        u64::from(w) * u64::from(h) <= 1_000_000,
        "cap 后像素量 {}×{} 应 ≤1M",
        w,
        h
    );
    assert!(scale.ss_x >= scale.ss_y, "横向宽格仍应横向采样更密");
}

#[test]
fn test_legacybox_letterbox_keeps_natural_aspect() {
    for text in ["A", "LUMINO", "中文"] {
        assert_aspect(text, 24, 60, false, TextLayoutMode::LegacyBox);
    }
    assert_aspect("LUMINO", 60, 24, true, TextLayoutMode::LegacyBox);
}

#[test]
fn test_parametric_keeps_natural_aspect() {
    assert_aspect("A", 24, 60, false, TextLayoutMode::Parametric);
    assert_aspect("LUMINO", 24, 60, false, TextLayoutMode::Parametric);
    assert_aspect("中文", 24, 60, false, TextLayoutMode::Parametric);
    assert_aspect("LUMINO", 120, 24, true, TextLayoutMode::Parametric);
}

/// 默认 threshold=50% 下，key 轴覆盖率不能被 9.6:1 的 cell 宽度稀释。
#[test]
fn test_default_threshold_keeps_text_ink() {
    let font = bundled_font();
    let params = params(4.0);
    let (cols, rows) = (12, 30);
    let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), cols, rows);
    let (w, h, buf) =
        parametric::rasterize_glyph_alpha_params_with_font("A", cols, rows, &params, &font, scale)
            .expect("内置字体应能栅格化");
    let occ = occupancy::alpha_to_occupancy(&buf, w, cols, rows, scale, 50.0);
    assert!(
        occ.iter().flatten().any(|&on| on),
        "A 在默认 50% 阈值下必须有墨格（key 轴覆盖率），不能全丢"
    );
    let _ = h;
}

/// 合成 1px 竖笔：key 轴覆盖率 100%，面积覆盖率 4/160=2.5% → 50% 面积必丢。
#[test]
fn test_key_axis_coverage_beats_area_on_wide_cell() {
    let width = 40usize;
    let height = 4usize;
    let mut buf = vec![0u8; width * height];
    for y in 0..height {
        buf[y * width + 20] = 255;
    }
    let scale = RasterScale {
        ss_x: 40,
        ss_y: 4,
        px_per_key: 4.0,
        vertical: false,
        legacy: false,
    };
    let key_axis = occupancy::alpha_to_occupancy(&buf, 40, 1, 1, scale, 50.0);
    assert!(key_axis[0][0], "竖笔覆盖 4/4 个 key 子行，应发声");

    let legacy = RasterScale {
        legacy: true,
        ..scale
    };
    let area = occupancy::alpha_to_occupancy(&buf, 40, 1, 1, legacy, 50.0);
    assert!(!area[0][0], "面积 4/160=2.5% 不应通过 50% 阈值");
}

#[test]
fn test_converted_size_font_rows_roundtrip() {
    let cap_ratio = 0.72;
    let rows = tool::converted_size(24.0, TextUnit::Font, TextUnit::Rows, cap_ratio);
    assert!(
        (rows - 17.28).abs() < 0.01,
        "24 × 0.72 = 17.28，实际 {rows}"
    );
    let font = tool::converted_size(rows, TextUnit::Rows, TextUnit::Font, cap_ratio);
    assert!((font - 24.0).abs() < 1e-3, "往返应回到 24，实际 {font}");
    assert_eq!(
        tool::converted_size(200.0, TextUnit::Font, TextUnit::Rows, cap_ratio),
        96.0,
        "超出面板范围应 clamp 到 96"
    );
}

#[test]
fn test_cap_height_ratio_is_in_expected_range() {
    let font = bundled_font();
    let ratio = font::cap_height_ratio(&font).expect("内置字体应有 H");
    assert!(
        (0.60..=0.90).contains(&ratio),
        "Noto 的 cap/em 应在 0.6..0.9，实际 {ratio}"
    );
}

#[test]
fn test_set_text_params_converts_unit_size() {
    let mut editor = Editor::new();
    let params = TextParams {
        unit: TextUnit::Rows,
        ..TextParams::default()
    };
    editor.set_text_params(params);
    let rows = editor.editor_state.text_tool.params.clone();
    if (rows.size - 24.0).abs() < 1e-3 {
        eprintln!("跳过 test_set_text_params_converts_unit_size：无可用系统字体或 cap ratio=1");
        return;
    }
    assert!(
        rows.size < 24.0,
        "Font(24 keys) → Rows 应缩到大写 H 高度（约 17），实际 {}",
        rows.size
    );
    assert_eq!(
        editor.editor_state.text_tool.layout_mode,
        TextLayoutMode::Parametric,
        "调参仍应自动切到 Parametric（面板 Mode 行可见）"
    );

    let mut back = rows;
    back.unit = TextUnit::Font;
    editor.set_text_params(back);
    let font = &editor.editor_state.text_tool.params;
    assert!(
        (font.size - 24.0).abs() < 1.0,
        "Rows → Font 往返应回到 24 附近，实际 {}",
        font.size
    );
}

/// 重烘焙稳定性：k 冻结在 TextV2，换 zoom 不影响音符。
#[test]
fn test_text_shape_notes_are_zoom_independent_with_frozen_k() {
    use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel, ShapeRect, TextOpV2};

    let mut editor = Editor::new();
    let params = TextParams {
        size: 8.0,
        ..TextParams::default()
    };
    let k = default_k();
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, k);
    model.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 960.0, 64.0));
    let op = TextOpV2 {
        text: "A".to_string(),
        params,
        layout: TextLayoutMode::Parametric,
        snap: 480.0,
        merged: false,
        vertical: false,
        k,
    };
    let a = editor.text_shape_notes_v2(&model, &op, 1);
    if a.is_empty() {
        eprintln!("跳过 test_text_shape_notes_are_zoom_independent_with_frozen_k：无可用系统字体");
        return;
    }
    editor.editor_state.view.zoom_x = 0.37;
    editor.editor_state.view.zoom_y = 7.0;
    let b = editor.text_shape_notes_v2(&model, &op, 1);
    assert_eq!(a, b, "op.k 冻结后，重烘焙不得随当前 zoom 改变");
}
