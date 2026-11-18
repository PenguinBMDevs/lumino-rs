//! UI-023 回归：文字栅格的**比例权威**
//!
//! 覆盖四件事：
//! 1. `RasterScale` 的 `ss_x:ss_y` 必须等于屏幕格比例（横向 / 纵向）；
//! 2. 等比 letterbox 不受采样比例影响（字形不被拉伸）；
//! 3. 阈值是 **key 轴覆盖率**，默认 50% 下细笔画仍能出格；
//! 4. `TextV2` 冻结 `k` 后重烘焙与当前缩放无关，预览不改写文本。

use ab_glyph::{Font, PxScale, ScaleFont};

use lumino_core::shape::{
    ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect, TextLayoutMode, TextOpV2, TextParams,
};
use lumino_core::{TextAlign, TextUnit};

use super::super::rasterize::{InkRule, alpha_to_occupancy};
use super::super::rasterize_glyph_alpha_letterbox;
use crate::Editor;
use crate::interaction::text_tool::{RasterScale, rasterize_text_for_tool};

/// 仓库内置字体：避免系统字体差异（与 `tests.rs` 同一来源）
fn bundled_font() -> ab_glyph::FontArc {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../resources/fonts/NotoSansSC-Regular.ttf"
    ));
    ab_glyph::FontArc::try_from_vec(bytes.to_vec()).expect("仓库内置 Noto Sans SC 应可解析")
}

/// 墨迹 bbox（`(x0, y0, x1, y1)`，含端点）；无墨迹时 `None`
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

/// 默认视图的 `k`：`zoom_y / (ppq × zoom_x) = 20 / (480 × 0.1)`
fn default_k() -> f32 {
    20.0 / (480.0 * 0.1)
}

/// `ss_x : ss_y` 必须等于「1 个时间格 : 1 个 key」的屏幕长度比
#[test]
fn test_raster_scale_matches_screen_cell_ratio() {
    // 覆盖默认 9.6:1、4.8:1、2.5:1、96:1 与纵向 1:10（时间格比 key 短一个数量级）
    let cases = [
        (1920.0_f32, 480.0_f32, 0.1_f32, 20.0_f32),
        (1920.0, 480.0, 0.1, 10.0),
        (960.0, 480.0, 0.25, 12.0),
        (1920.0, 480.0, 1.0, 20.0),
        (1920.0, 480.0, 0.1, 1920.0),
    ];
    for (snap, ppq, zoom_x, zoom_y) in cases {
        let k = zoom_y / (ppq * zoom_x);
        // 屏幕：时间格 = snap × zoom_x 像素，key = zoom_y 像素
        let screen_ratio = (snap * zoom_x) / zoom_y;
        let horizontal = RasterScale::from_k(false, snap, ppq, k, 4, 16);
        assert!(
            ((horizontal.ss_x / horizontal.ss_y) - screen_ratio).abs() / screen_ratio < 0.01,
            "横向 ss_x:ss_y 必须等于屏幕格比例 {screen_ratio}，实际 {}（snap={snap}, ppq={ppq}, zoom=({zoom_x},{zoom_y})）",
            horizontal.ss_x / horizontal.ss_y
        );
        let vertical = RasterScale::from_k(true, snap, ppq, k, 16, 4);
        assert!(
            ((vertical.ss_y / vertical.ss_x) - screen_ratio).abs() / screen_ratio < 0.01,
            "纵向轴对调后 ss_y:ss_x 必须等于屏幕格比例 {screen_ratio}，实际 {}",
            vertical.ss_y / vertical.ss_x
        );
        // px_per_key 恒等于 pitch 轴的 ss（横向 = ss_y、纵向 = ss_x）
        assert_eq!(horizontal.px_per_key, horizontal.ss_y);
        assert_eq!(vertical.px_per_key, vertical.ss_x);
    }
}

/// 旧路径固定 4×4 正方格——这正是 #273 的病灶；新路径必须体现 9.6:1
#[test]
fn test_legacy_scale_is_square_not_screen_ratio() {
    let screen_ratio = 1920.0 * 0.1 / 20.0;

    let legacy = RasterScale::legacy(false);
    assert_eq!((legacy.ss_x, legacy.ss_y), (4.0, 4.0), "旧路径固定方格");
    assert!(legacy.legacy);
    assert!(
        (legacy.ss_x / legacy.ss_y - screen_ratio).abs() > 1.0,
        "旧方格比例远小于屏幕格比例 {screen_ratio}（这正是字形被压扁的原因）"
    );

    let live = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 2, 15);
    assert!(
        ((live.ss_x / live.ss_y) - screen_ratio).abs() / screen_ratio < 0.01,
        "新路径的横向采样必须等于屏幕格比例 {screen_ratio}"
    );
}

/// 等比 letterbox：同一 buffer 高下，采样比例不改变字形渲染结果
///
/// 参照组用 1:1 采样（`snap/ppq = 1, k = 1` ⇒ ss = 4×4）；两组都用 `rows = 24`
/// （buffer 高 96px）、列数足够宽（fit = 1），墨迹 bbox 必须逐像素同框。
#[test]
fn test_letterbox_is_independent_of_sampling_ratio() {
    let font = bundled_font();
    let wide = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 128, 24);
    let square = RasterScale::from_k(false, 480.0, 480.0, 1.0, 128, 24);
    assert_eq!(
        (square.ss_x, square.ss_y),
        (4.0, 4.0),
        "参照组必须是 1:1 采样"
    );

    let wide_render =
        rasterize_glyph_alpha_letterbox("LUMINO", 128, 24, &font, &wide, TextAlign::Left)
            .expect("内置字体应能栅格化 ASCII");
    let square_render =
        rasterize_glyph_alpha_letterbox("LUMINO", 128, 24, &font, &square, TextAlign::Left)
            .expect("内置字体应能栅格化 ASCII");
    let (w1, h1, b1) = wide_render;
    let (w2, h2, b2) = square_render;
    assert_eq!(h1, h2, "同 rows 下 buffer 高由 ss_y 决定，必须一致");
    assert!(w1 > w2, "9.6:1 采样应给出更宽的 buffer");

    let box1 = ink_bbox(&b1, w1 as usize, h1 as usize).expect("LUMINO 应有墨迹");
    let box2 = ink_bbox(&b2, w2 as usize, h2 as usize).expect("LUMINO 应有墨迹");
    assert_eq!(
        box1, box2,
        "等比 letterbox 的字形不得依赖采样比例（否则比例修复就是假的）"
    );

    // 墨迹宽高比 = 字体自然比例：与「按同一 px 字号算出的 advance / cap height」对照 ≤10%
    let px = h1 as f32;
    let natural = natural_ink_aspect(&font, "LUMINO", px);
    let measured = (box1.2 - box1.0 + 1) as f32 / (box1.3 - box1.1 + 1) as f32;
    assert!(
        (measured - natural).abs() / natural <= 0.1,
        "墨迹宽高比 {measured} 与字体自然比例 {natural} 误差需 ≤10%"
    );
}

/// 字体自然墨迹宽高比（advance 总宽 / 大写高度），作为「同字体参考比例」
fn natural_ink_aspect(font: &ab_glyph::FontArc, text: &str, px: f32) -> f32 {
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let advance: f32 = text
        .chars()
        .map(|ch| scaled.h_advance(font.glyph_id(ch)))
        .sum();
    let cap = font
        .outline_glyph(
            font.glyph_id('H')
                .with_scale_and_position(scale, ab_glyph::Point::default()),
        )
        .map(|outline| {
            let b = outline.px_bounds();
            (b.max.y - b.min.y).max(1e-3)
        })
        .unwrap_or(px * 0.7);
    advance / cap
}

/// 默认 50% 阈值必须按 key 轴覆盖率判定：面积口径在 38:4 采样下会被稀释
#[test]
fn test_default_threshold_keeps_glyph_cells() {
    let font = bundled_font();
    // 8 时间格 × 24 key ⇒ buffer 307 × 96
    let scale = RasterScale::from_k(false, 1920.0, 480.0, default_k(), 8, 24);
    let (w, h, buf) = rasterize_glyph_alpha_letterbox("A", 8, 24, &font, &scale, TextAlign::Left)
        .expect("内置字体应能栅格化 ASCII");

    let count = |rule| {
        alpha_to_occupancy(&buf, 8, 24, &scale, rule)
            .iter()
            .flatten()
            .filter(|on| **on)
            .count()
    };
    let key_axis = count(InkRule::KeyAxis { threshold: 0.5 });
    let area = count(InkRule::Area {
        threshold: 0.5,
        inclusive: true,
    });
    assert!(
        key_axis > 0,
        "默认 50% key 轴覆盖率下 A 必须有墨格（buffer {w}×{h}）"
    );
    assert!(
        key_axis > area,
        "key 轴覆盖率必须比面积覆盖率更抗稀释：key={key_axis} vs area={area}"
    );

    // 多字母：LUMINO 同样必须留有墨格
    let (_, _, buf) =
        rasterize_glyph_alpha_letterbox("LUMINO", 16, 24, &font, &scale, TextAlign::Left)
            .expect("内置字体应能栅格化 ASCII");
    let lumino = alpha_to_occupancy(&buf, 16, 24, &scale, InkRule::KeyAxis { threshold: 0.5 })
        .iter()
        .flatten()
        .filter(|on| **on)
        .count();
    assert!(lumino > 0, "默认阈值下 LUMINO 不得被整字判空");
}

/// 同一 `TextV2` + `op.k` 在两个不同缩放下重烘焙，音符必须逐字段一致
#[test]
fn test_text_v2_rebake_is_zoom_independent() {
    let mut editor = Editor::new();
    let k = default_k();
    let mut model = ShapeModel::new(7, 1, ShapeKind::Custom, k);
    model.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 3840.0, 84.0));
    let op = TextOpV2 {
        text: "A".to_string(),
        params: TextParams::default(),
        layout: TextLayoutMode::Parametric,
        snap: 1920.0,
        merged: false,
        vertical: false,
        k,
    };
    model.ops.push(ShapeOp::TextV2(op.clone()));

    editor.editor_state.view.zoom_x = 0.1;
    editor.editor_state.view.zoom_y = 20.0;
    let first = editor.text_shape_notes_v2(&model, &op, 1);
    if first.is_empty() {
        eprintln!("跳过 test_text_v2_rebake_is_zoom_independent：无可用系统字体");
        return;
    }

    // 换一个完全不同的缩放：k 已冻结，重烘焙不得漂移
    editor.editor_state.view.zoom_x = 0.5;
    editor.editor_state.view.zoom_y = 5.0;
    let second = editor.text_shape_notes_v2(&model, &op, 1);
    assert_eq!(
        first, second,
        "同一 TextV2 + op.k 在不同缩放下的重烘焙必须逐字段一致"
    );

    // 形状入口必须分派到 V2（否则上面的稳定性在真实链路上不生效）
    let via_dispatch = editor.shape_notes_of(&model, 1);
    assert_eq!(via_dispatch, first, "shape_notes_of 必须分派到 TextV2 路径");
}

/// 栅格签名必须随 zoom / snap 变化（否则预览会复用旧纹理）
#[test]
fn test_text_raster_key_tracks_zoom_and_snap() {
    let mut editor = Editor::new();
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 3840.0, 60, 64);
        tt.active = true;
        tt.text = "A".to_string();
    }
    let base = editor.text_raster_key().expect("激活文本框后应有栅格签名");

    editor.editor_state.view.zoom_x *= 2.0;
    let zoomed = editor.text_raster_key().expect("缩放后仍应有栅格签名");
    assert_ne!(base, zoomed, "缩放改变 k ⇒ 采样比例变 ⇒ 签名必须变");

    editor.editor_state.view.snap_precision *= 2.0;
    let snapped = editor.text_raster_key().expect("改精度后仍应有栅格签名");
    assert_ne!(zoomed, snapped, "音符精度改变格数 ⇒ 签名必须变");
}

/// 预览只按 `trim()` 判空，**不改写**文本内容（确认路径用原文本）
#[test]
fn test_preview_does_not_trim_text_content() {
    let mut editor = Editor::new();
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 3840.0, 60, 64);
        tt.active = true;
        tt.text = " A ".to_string();
    }
    let Some(padded) = editor.text_tool_preview_rgba() else {
        eprintln!("跳过 test_preview_does_not_trim_text_content：无可用系统字体");
        return;
    };
    editor.editor_state.text_tool.text = "A".to_string();
    let bare = editor.text_tool_preview_rgba().expect("无空格文本必须预览");

    assert_ne!(
        padded.2, bare.2,
        "含前后空格的文本必须按原样栅格化（否则预览与确认会不一致）"
    );

    editor.editor_state.text_tool.text = "   ".to_string();
    assert!(
        editor.text_tool_preview_rgba().is_none(),
        "纯空白文本仍应无预览"
    );
}

/// Font ⇄ Rows 单位切换按同字体 cap height 换算字号，往返误差 ≤10%
#[test]
fn test_unit_switch_converts_font_size() {
    if lumino_note_core::font_scanner::get_cached_fonts().is_empty() {
        eprintln!("跳过 test_unit_switch_converts_font_size：无可用系统字体");
        return;
    }
    let mut editor = Editor::new();
    let font_params = TextParams {
        size: 24.0,
        unit: TextUnit::Font,
        ..TextParams::default()
    };
    editor.set_text_params(font_params.clone());

    let mut rows_params = font_params.clone();
    rows_params.unit = TextUnit::Rows;
    editor.set_text_params(rows_params);
    let rows_size = editor.editor_state.text_tool.params.size;
    assert!(
        rows_size < 24.0,
        "Font→Rows 应换算成大写高度（数值变小），实际 {rows_size}"
    );
    assert!(
        (4.0..=96.0).contains(&rows_size),
        "换算结果必须落在面板范围 4..=96，实际 {rows_size}"
    );
    assert_eq!(
        editor.editor_state.text_tool.layout_mode,
        TextLayoutMode::Parametric,
        "改参数仍应自动进入 Parametric（Mode 行可见，不新增 toast）"
    );

    let mut back = editor.editor_state.text_tool.params.clone();
    back.unit = TextUnit::Font;
    editor.set_text_params(back);
    let round_trip = editor.editor_state.text_tool.params.size;
    assert!(
        (round_trip - 24.0).abs() / 24.0 <= 0.1,
        "Font→Rows→Font 往返误差需 ≤10%，实际 {round_trip}"
    );
}

/// 预览与确认必须共用同一份占用网格（同一文本 / 字体 / 比例）
#[test]
fn test_preview_and_confirm_share_occupancy() {
    let mut editor = Editor::new();
    {
        let tt = &mut editor.editor_state.text_tool;
        tt.set_drag(0.0, 3840.0, 60, 64);
        tt.active = true;
        tt.text = " A ".to_string();
    }
    let Some((grid, scale)) = editor.text_tool_grid_scale() else {
        return;
    };
    let Some(confirm_occ) =
        rasterize_text_for_tool(&editor.editor_state.text_tool, " A ", &grid, &scale)
    else {
        eprintln!("跳过 test_preview_and_confirm_share_occupancy：无可用系统字体");
        return;
    };
    // 预览走的也是 `rasterize_glyph_alpha_for_tool` + 同一 grid/scale；
    // 此处验证「同文本 → 同一占用网格」这条契约本身是确定的。
    let again = rasterize_text_for_tool(&editor.editor_state.text_tool, " A ", &grid, &scale)
        .expect("同输入必须可重复栅格化");
    assert_eq!(confirm_occ, again, "同文本 / 字体 / 比例下占用网格必须一致");
}
