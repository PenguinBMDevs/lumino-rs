//! 走带覆盖层单测：ghost 的 Y/高度/取色必须与真实音符着色器同口径。

use super::*;
use crate::arrangement_renderer::ARRANGEMENT_NOTE_HEIGHT;

/// 测试视口：单泳道高 48px、无滚动、无缩放、画布偏移为 0。
fn viewport() -> ArrangementViewport {
    ArrangementViewport {
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x: 200.0,
        zoom_y: 1.0,
        track_height: 48.0,
        canvas_offset: [0.0, 0.0],
        canvas_size: [800.0, 600.0],
        total_ticks: 7680,
        ppq: 480,
    }
}

fn colors() -> ArrangementViewColors {
    ArrangementViewColors {
        bg: [0.0, 0.0, 0.0],
        lane_even: [0.0, 0.0, 0.0],
        lane_odd: [0.0, 0.0, 0.0],
        measure_line: [0.0, 0.0, 0.0, 1.0],
        playhead: [0.0, 0.0, 0.0, 1.0],
        sel_rect: [0.0, 0.0, 0.0],
    }
}

/// 构建前景覆盖层并只保留 ghost 实例（tag 5）。
///
/// `playback_position = 0.0`、`sel_rect` / `drag_sel_rect` 均为 `None`，
/// 因此产出物里除 ghost 外不会有其它实例。
#[allow(clippy::too_many_arguments)]
fn build_ghosts(
    viewport: &ArrangementViewport,
    colors: &ArrangementViewColors,
    track_order: &[usize],
    track_colors: &[[f32; 3]],
    track_visible: &[bool],
    ghost_notes: &[(f64, f64, usize, u8)],
    note_height: f32,
) -> Vec<ArrangementNoteInstance> {
    let params = ArrangementSceneParams {
        viewport,
        track_order,
        track_colors,
        track_visible,
        midi_doc: None,
        playback_position: 0.0,
        colors,
        note_height,
        ghost_notes,
        sel_rect: None,
        drag_sel_rect: None,
        time_signatures: &[],
    };
    let mut out = Vec::new();
    build_arrangement_overlay_front(&mut out, &params);
    out.retain(|i| i.tag == 5);
    out
}

/// 解包 `rgba_packed` → `(r8, g8, b8, a8)`。
fn unpack_rgba(instance: &ArrangementNoteInstance) -> (u8, u8, u8, u8) {
    (
        (instance.rgba_packed & 0xFF) as u8,
        ((instance.rgba_packed >> 8) & 0xFF) as u8,
        ((instance.rgba_packed >> 16) & 0xFF) as u8,
        ((instance.rgba_packed >> 24) & 0xFF) as u8,
    )
}

/// ghost 的 Y/高度必须逐字复刻 `shaders/arrangement_note.wgsl`：
/// `key_h = lane_height/128`、`note_y = lane_top + (127-key)*key_h + key_h*0.5`、
/// `sy = note_y - half_h`、`sh = 2*half_h`。
///
/// 旧实现写死 `lane_top + lh*0.5 - 2.0`（本例 = 22.0），本用例就是防它回归。
#[test]
fn test_ghost_y_and_height_follow_real_note_formula() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize];
    let palette = [[0.0f32, 0.0, 0.0]];
    let visible = [true];
    let ghosts = [(0.0, 480.0, 0, 60u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert_eq!(out.len(), 1, "应产出 1 个 ghost 实例");
    let ghost = &out[0];
    let lane_y = 0.0_f32;
    let key_h = vp.track_height * vp.zoom_y / 128.0;
    let expected_y = lane_y + (127.0 - 60.0) * key_h + key_h * 0.5 - ARRANGEMENT_NOTE_HEIGHT * 0.5;

    assert!(
        (ghost.y - expected_y).abs() < 1e-4,
        "ghost Y 应为 {expected_y}（真实音符公式），实际 {}",
        ghost.y
    );
    assert!(
        (ghost.h - ARRANGEMENT_NOTE_HEIGHT).abs() < 1e-4,
        "ghost 高度应等于音符条高度 {}，实际 {}",
        ARRANGEMENT_NOTE_HEIGHT,
        ghost.h
    );
    assert!(
        (ghost.y - (lane_y + vp.track_height * vp.zoom_y * 0.5 - 2.0)).abs() > 1.0,
        "不得回归到泳道中线死值 lane_y + lh*0.5 - 2.0"
    );
}

/// 多 pitch 选区不得塌到同一条线上：相邻半音差 = key_h，本例 12 个半音。
#[test]
fn test_ghost_multi_pitch_spans_lane_vertically() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize];
    let palette = [[0.0f32, 0.0, 0.0]];
    let visible = [true];
    let ghosts = [(0.0, 480.0, 0, 60u8), (0.0, 480.0, 0, 72u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert_eq!(out.len(), 2);
    let key_h = vp.track_height * vp.zoom_y / 128.0;
    // 高音在上：key 72 的 Y 比 key 60 小 12*key_h
    let dy = out[0].y - out[1].y;
    assert!(
        (dy - 12.0 * key_h).abs() < 1e-4,
        "高 12 个半音应上移 12*key_h = {}，实际 {dy}",
        12.0 * key_h
    );
}

/// 高度必须取自 `note_height` 参数（不得硬编码 4.0）：换 8.0 时 Y/高度同步变。
#[test]
fn test_ghost_height_follows_note_height_param() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize];
    let palette = [[0.0f32, 0.0, 0.0]];
    let visible = [true];
    let ghosts = [(0.0, 480.0, 0, 60u8)];

    let out = build_ghosts(&vp, &colors, &order, &palette, &visible, &ghosts, 8.0);

    assert!((out[0].h - 8.0).abs() < 1e-4, "高度应跟随 note_height 参数");
    let key_h = vp.track_height * vp.zoom_y / 128.0;
    let expected_y = (127.0 - 60.0) * key_h + key_h * 0.5 - 4.0;
    assert!(
        (out[0].y - expected_y).abs() < 1e-4,
        "half_h 随高度变化，Y 必须同步，期望 {expected_y}，实际 {}",
        out[0].y
    );
}

/// 取色与真实音符同源：调色板色 × `BORDER_DARKEN_FACTOR`、alpha 1.0、无圆角无边框。
///
/// 真实音符在走带里通体按描边色渲染（4px 高 + 1px 边框 → hy = 0.5），
/// 故白 1.0 的轨道色最终落在 1.0 × 0.4 × 255 + 0.5 = 102。
/// 若与 shader 同步调整了加深系数，本断言需一并更新。
#[test]
fn test_ghost_color_matches_real_note_rendering() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize];
    let palette = [[1.0f32, 0.5, 0.25]];
    let visible = [true];
    let ghosts = [(0.0, 480.0, 0, 60u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert_eq!(
        unpack_rgba(&out[0]),
        (102, 51, 26, 255),
        "色/alpha 必须与真实音符一致（调色板色 × 0.4、a = 1.0）"
    );
    assert_eq!(
        out[0].props_packed, 0,
        "不得带圆角/边框（真实音符是硬边矩形，且覆盖层边框会造成二次压暗）"
    );
}

/// 侧栏排序后 `track_order` 非恒等：ghost 在**视觉轨 0**，颜色必须取
/// **文档轨 1** 的调色板色（音符层按文档轨取色）。按视觉轨取色会串色。
#[test]
fn test_ghost_color_uses_document_track_not_visual_lane() {
    let vp = viewport();
    let colors = colors();
    let order = [1usize, 0];
    let palette = [[0.1f32, 0.1, 0.1], [1.0, 1.0, 1.0]];
    let visible = [true, true];
    let ghosts = [(0.0, 480.0, 0, 60u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert_eq!(out.len(), 1);
    assert_eq!(
        unpack_rgba(&out[0]),
        (102, 102, 102, 255),
        "视觉轨 0 对应文档轨 1（白色），取色必须经 track_order 映射"
    );
    // 泳道定位仍按视觉轨 0：不含泳道偏移
    let key_h = vp.track_height * vp.zoom_y / 128.0;
    let expected_y = (127.0 - 60.0) * key_h + key_h * 0.5 - ARRANGEMENT_NOTE_HEIGHT * 0.5;
    assert!((out[0].y - expected_y).abs() < 1e-4, "泳道须按视觉位置定位");
}

/// 静音/隐藏轨不画 ghost：音符层在该轨被 `lane_index = f32::MAX` 哨兵剔除，
/// lane 背景循环同样跳过——否则 ghost 会悬在没有内容的空泳道上。
#[test]
fn test_ghost_skips_muted_lane() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize, 1];
    let palette = [[1.0f32, 1.0, 1.0], [1.0, 1.0, 1.0]];
    let visible = [true, false];
    let ghosts = [(0.0, 480.0, 1, 60u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert!(out.is_empty(), "静音轨上不得绘制 ghost");
}

/// 越界泳道 / 取色表越界一律安全丢弃（不得 panic，也不得画出幽灵泳道）。
#[test]
fn test_ghost_out_of_range_lane_is_dropped() {
    let vp = viewport();
    let colors = colors();
    let order = [0usize];
    let palette = [[1.0f32, 1.0, 1.0]];
    let visible = [true];

    let lane_beyond_track_order = [(0.0, 480.0, 7, 60u8)];
    assert!(
        build_ghosts(
            &vp,
            &colors,
            &order,
            &palette,
            &visible,
            &lane_beyond_track_order,
            ARRANGEMENT_NOTE_HEIGHT,
        )
        .is_empty(),
        "视觉轨越界应丢弃"
    );

    let doc_track_beyond_palette = [(0.0, 480.0, 0, 60u8)];
    let empty_palette: [[f32; 3]; 0] = [];
    assert!(
        build_ghosts(
            &vp,
            &colors,
            &order,
            &empty_palette,
            &visible,
            &doc_track_beyond_palette,
            ARRANGEMENT_NOTE_HEIGHT,
        )
        .is_empty(),
        "取色表缺该文档轨时应丢弃，不得越界"
    );
}

/// 最小宽度与真实音符一致：`arrangement_note.wgsl` 用 `max(length*ppu, 1.0)`，
/// 覆盖层不得再夹到 2.0。
#[test]
fn test_ghost_min_width_matches_real_note() {
    let mut vp = viewport();
    vp.zoom_x = 1.0; // 0.5 tick → 0.5px，触发最小宽度夹取
    let colors = colors();
    let order = [0usize];
    let palette = [[1.0f32, 1.0, 1.0]];
    let visible = [true];
    let ghosts = [(10.0, 10.5, 0, 60u8)];

    let out = build_ghosts(
        &vp,
        &colors,
        &order,
        &palette,
        &visible,
        &ghosts,
        ARRANGEMENT_NOTE_HEIGHT,
    );

    assert_eq!(out.len(), 1);
    assert!(
        (out[0].w - 1.0).abs() < 1e-6,
        "最小宽度应为 1.0（与真实音符一致），实际 {}",
        out[0].w
    );
}
