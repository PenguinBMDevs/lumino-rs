//! 画笔工具单测 — 拖动/命中/批量与渲染口径（从 tests.rs 拆出）

use super::*;

// ── 拖动笔画 ─────────────────────────

#[test]
fn test_press_on_stroke_body_starts_drag_not_new_stroke() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);

    let body = editor.line_pos_screen_pos((1200.0, 60.0));
    editor.handle_brush_pressed(body, None, 60);
    assert!(
        editor.editor_state.brush_tool.is_dragging(),
        "命中实心区应进入拖动"
    );
    assert_eq!(
        editor.editor_state.brush_tool.strokes.len(),
        1,
        "不得新建笔画"
    );

    let target = editor.line_pos_screen_pos((1337.0, 62.4));
    editor.handle_brush_moved(target);
    editor.handle_released();

    let points = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert_eq!(points[0].1, 62.0, "Y 向按单个 key 吸附");
    assert!(
        (points[0].0 - 137.0).abs() < 1.0,
        "X 向自由：{:?}",
        points[0]
    );
    assert!(!editor.editor_state.brush_tool.is_active());
    assert!(
        editor.editor_state.brush_tool.has_pending(),
        "拖动后仍待确认"
    );
}

#[test]
fn test_dragged_position_is_what_confirm_generates() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);
    let body = editor.line_pos_screen_pos((1200.0, 60.0));
    editor.handle_brush_pressed(body, None, 60);
    let target = editor.line_pos_screen_pos((1337.0, 62.4));
    editor.handle_brush_moved(target);
    editor.handle_released();

    let points = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert!(editor.confirm_brush());
    let mut cells: Vec<cov::CoveredCell> = notes_of(&editor, 1)
        .iter()
        .map(|&(tick, key)| ((tick / 240.0) as i64, key as u16))
        .collect();
    // 生成结果 = 拖动后点列的覆盖集（预览与生成同口径）
    let mut expected = cov::cover_cells(&points, 240.0);
    cells.sort_unstable();
    expected.sort_unstable();
    assert_eq!(cells, expected, "√ 生成位置与预览一致");
}

// ── 笔画历史（独立于 document） ─────────────────────────

#[test]
fn test_stroke_history_undo_redo_before_confirm() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (240.0, 60.0)]);
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 1);

    assert!(editor.undo(), "撤销笔画编辑");
    assert!(editor.editor_state.brush_tool.strokes.is_empty());
    assert!(!editor.can_undo(), "已无更多历史（document 亦无）");

    assert!(editor.redo(), "重做笔画编辑");
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 1);
    assert!(!editor.can_redo());
}

// ── 颜色与宽度 ─────────────────────────

#[test]
fn test_darken_is_40_percent() {
    let color = darken(
        iced_core::Color::from_rgba(1.0, 0.5, 0.25, 1.0),
        LAYER_DARKEN,
    );
    assert!((color.r - 0.6).abs() < 1e-6, "1.0 × 0.6");
    assert!((color.g - 0.3).abs() < 1e-6, "0.5 × 0.6");
    assert!((color.b - 0.15).abs() < 1e-6, "0.25 × 0.6");
    assert_eq!(color.a, 1.0, "alpha 不变");
}

#[test]
fn test_layer_color_comes_from_assigned_track_display_color() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 4, 1, &[]);
    editor.brush.set_thickness(2);
    editor.brush.set_track(0, Some(1));
    editor.brush.set_track(1, Some(3));

    let expected0 = {
        let c = lumino_extras::palette::current_track_color_f32(1);
        darken(
            iced_core::Color::from_rgba(c[0], c[1], c[2], c[3]),
            LAYER_DARKEN,
        )
    };
    let expected1 = {
        let c = lumino_extras::palette::current_track_color_f32(3);
        darken(
            iced_core::Color::from_rgba(c[0], c[1], c[2], c[3]),
            LAYER_DARKEN,
        )
    };
    let color0 = editor.brush_track_color(editor.brush_track_for_level(0, 1));
    let color1 = editor.brush_track_color(editor.brush_track_for_level(1, 1));
    assert!((color0.r - expected0.r).abs() < 1e-6 && (color0.g - expected0.g).abs() < 1e-6);
    assert!((color1.r - expected1.r).abs() < 1e-6 && (color1.g - expected1.g).abs() < 1e-6);
    assert!(
        color0.r != color1.r || color0.g != color1.g || color0.b != color1.b,
        "不同轨层色不同（硬切换，非渐变）"
    );
}

#[test]
fn test_total_width_follows_zoom_and_thickness() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(3);
    editor.editor_state.view.zoom_y = 20.0;
    assert!(
        (editor.brush_total_width_px() - 60.0).abs() < 1e-6,
        "3 × 20px"
    );

    editor.editor_state.view.zoom_y = 8.0;
    assert!(
        (editor.brush_total_width_px() - 24.0).abs() < 1e-6,
        "缩放后实时重算"
    );

    editor.brush.set_thickness(1);
    assert!(
        (editor.brush_total_width_px() - 8.0).abs() < 1e-6,
        "粗细度 1 = 单 key 高"
    );
}

// ── 批量归并路径（大规模 √） ─────────────────────────

#[test]
fn test_large_confirm_uses_batch_path_and_is_complete() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(20);
    // 151 格 × 20 层 = 3020 音符 > BATCH_INSERT_THRESHOLD → 走批量归并
    seed_stroke(&mut editor, &[(0.0, 60.0), (150.0 * 240.0, 60.0)]);
    assert!(editor.confirm_brush());

    let notes = notes_of(&editor, 1);
    assert_eq!(notes.len(), 151 * 20, "批量路径不得丢音符");
    assert!(notes.contains(&(0.0, 60u8)) && notes.contains(&(36000.0, 79u8)));

    assert!(editor.undo(), "批量路径同样只占一条历史记录");
    assert!(notes_of(&editor, 1).is_empty());
}

// ── √/× 按钮定位 ─────────────────────────

#[test]
fn test_button_rects_only_for_brush_with_pending_strokes() {
    let mut editor = brush_editor();
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "无笔画不显示按钮"
    );
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    assert!(crate::grid::brush_tool_box::brush_button_rects(&editor).is_some());

    editor.set_tool(Tool::Pencil);
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "非画刷工具不显示按钮"
    );
}

#[test]
fn test_hit_band_aligns_with_visual_blocks() {
    // 粗细度 5：方块占据 [落笔 key, 落笔 key+5)（向上铺）
    let mut editor = brush_editor();
    editor.brush.set_thickness(5);
    seed_stroke(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);

    // 方块带中心（key + 2）附近：必须命中
    let band_center = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY + 2.0));
    assert!(
        editor.brush_stroke_hit_test(band_center).is_some(),
        "方块带中心应命中"
    );
    // 方块带下方（落笔 key - 3，远离方块）不得命中
    let below = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY - 3.0));
    assert!(
        editor.brush_stroke_hit_test(below).is_none(),
        "方块带之外不得命中（所见即所按）"
    );
}
