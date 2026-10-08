//! 画笔工具单测 — 基础笔画/确认/图层（从 tests.rs 拆出）

use super::*;

#[test]
fn test_fast_drag_one_frame_covers_every_cell_on_confirm() {
    let mut editor = brush_editor();
    // 一帧从第 0 格跨到第 10 格（旧实现只盖首尾两格 → 中间 9 格空洞）
    seed_stroke(&mut editor, &[(0.0, 60.0), (2400.0, 60.0)]);
    assert!(editor.confirm_brush());

    let notes = notes_of(&editor, 1);
    let ticks: Vec<f32> = notes.iter().map(|n| n.0).collect();
    assert_eq!(
        ticks,
        (0..=10).map(|i| i as f32 * 240.0).collect::<Vec<f32>>(),
        "快速拖动必须逐格生成，无断墨空洞"
    );
    assert!(notes.iter().all(|n| n.1 == 60), "同 key 水平笔画");
}

#[test]
fn test_steep_drag_is_connected_on_confirm() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 66.0)]);
    assert!(editor.confirm_brush());

    let mut cells: Vec<cov::CoveredCell> = notes_of(&editor, 1)
        .iter()
        .map(|&(tick, key)| ((tick / 240.0) as i64, key as u16))
        .collect();
    // 覆盖集（单层）应当与覆盖函数一致：即"生成 = 预览所用覆盖集"
    let mut expected = cov::cover_cells(&[(0.0, 60.0), (960.0, 66.0)], 240.0);
    cells.sort_unstable();
    expected.sort_unstable();
    assert_eq!(cells, expected, "生成音符格集合 == 覆盖函数输出");
    assert!(cells.contains(&(0, 60)) && cells.contains(&(4, 66)));
}

// ── 拖动期间不写 document ─────────────────────────

#[test]
fn test_drawing_writes_nothing_to_document() {
    let mut editor = brush_editor();
    let start = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_brush_pressed(start, None, 60);
    for step in 1..=8 {
        let pos = editor.line_pos_screen_pos((step as f32 * 480.0, 60.0));
        editor.handle_brush_moved(pos);
    }
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "落笔拖动全程不得写入 document"
    );
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "未确认前不得产生 document 历史"
    );
    editor.handle_released();
    assert!(
        editor.editor_state.brush_tool.has_pending(),
        "松手进入待确认"
    );
    assert!(editor.can_undo(), "笔画编辑历史独立可用");
}

// ── √ 一次历史记录 ─────────────────────────

#[test]
fn test_confirm_creates_one_history_entry_undone_at_once() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(3);
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 60.0)]);
    seed_stroke(&mut editor, &[(0.0, 70.0), (960.0, 70.0)]);
    assert!(editor.confirm_brush());

    let before = notes_of(&editor, 1).len();
    assert!(before > 0, "√ 应生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "确认后清空笔画"
    );

    assert!(editor.undo(), "Ctrl+Z 一次回退全部生成音符");
    assert!(
        notes_of(&editor, 1).is_empty(),
        "一次撤销必须清掉全部笔画生成的全部音符（{before} 个）"
    );
    assert!(!editor.editor_state.data.history.can_undo(), "恰好一条记录");
}

#[test]
fn test_cancel_discards_all_strokes_without_history() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (960.0, 60.0)]);
    seed_stroke(&mut editor, &[(0.0, 70.0), (960.0, 70.0)]);
    editor.cancel_brush();

    assert!(!editor.editor_state.brush_tool.has_pending());
    assert!(notes_of(&editor, 1).is_empty(), "× 不得生成音符");
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "× 不得产生历史"
    );
}

// ── 层音轨分配 ─────────────────────────

#[test]
fn test_layer_tracks_follow_explicit_config() {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 4, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.brush.set_thickness(3);
    editor.brush.set_track(0, Some(1));
    editor.brush.set_track(1, Some(2));
    editor.brush.set_track(2, Some(3));

    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    assert!(editor.confirm_brush());

    assert_eq!(notes_of(&editor, 1), vec![(0.0, 60u8)], "层 0 → 轨 1");
    assert_eq!(notes_of(&editor, 2), vec![(0.0, 61u8)], "层 1 → 轨 2");
    assert_eq!(notes_of(&editor, 3), vec![(0.0, 62u8)], "层 2 → 轨 3");
}

#[test]
fn test_default_layer_assignment_uses_stroke_base_track() {
    // 4 轨（3 条普通轨）：落笔在轨 1，随后切到轨 2 —— 目标轨必须仍按落笔基准轨解析
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 4, 1, &[]);
    editor.editor_state.view.snap_precision = 240.0;
    editor.brush.set_thickness(2);

    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.editor_state.data.current_track = 2; // 画完切轨（预览/生成口径一致）
    assert!(editor.confirm_brush());

    assert_eq!(notes_of(&editor, 1), vec![(0.0, 60u8)], "层 0 回到基准轨 1");
    assert_eq!(notes_of(&editor, 2), vec![(0.0, 61u8)], "层 1 = 基准轨 +1");
    assert!(notes_of(&editor, 3).is_empty(), "不得按新当前轨分配");
}

#[test]
fn test_thickness_expands_upward_and_clamps_at_255() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(4);
    seed_stroke(&mut editor, &[(0.0, 253.0)]);
    assert!(editor.confirm_brush());
    let keys: Vec<u8> = notes_of(&editor, 1).iter().map(|n| n.1).collect();
    assert_eq!(keys, vec![253, 254, 255], "向上展开并截断到 255");
}

// ── 多笔画 / Conductor / 切工具 ─────────────────────────

#[test]
fn test_multiple_strokes_share_one_button_pair_and_apply_together() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    seed_stroke(&mut editor, &[(480.0, 64.0)]);
    seed_stroke(&mut editor, &[(960.0, 68.0)]);
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 3);

    let btns = crate::grid::brush_tool_box::brush_button_rects(&editor).expect("应有一对按钮");
    assert!(btns.confirm.width > 0.0 && btns.cancel.width > 0.0);

    assert!(editor.confirm_brush());
    assert_eq!(notes_of(&editor, 1).len(), 3, "√ 一次性应用全部笔画");
    assert!(editor.editor_state.brush_tool.strokes.is_empty());
}

#[test]
fn test_conductor_track_blocks_new_stroke() {
    let mut editor = brush_editor();
    editor.editor_state.data.current_track = 0;
    let pos = editor.line_pos_screen_pos((0.0, 60.0));
    editor.handle_brush_pressed(pos, None, 60);
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "Conductor 轨禁止落笔（与曲线工具对齐）"
    );
}

#[test]
fn test_pending_strokes_survive_track_switch_to_conductor() {
    // 待确认期间切到 Conductor：按钮与 √/× 仍可用（否则笔画被卡死）
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.editor_state.data.current_track = 0;
    assert!(crate::grid::brush_tool_box::brush_button_rects(&editor).is_some());
    assert!(editor.confirm_brush(), "待确认笔画应能正常确认");
    assert_eq!(notes_of(&editor, 1).len(), 1);
}

#[test]
fn test_switch_tool_discards_pending_strokes() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    editor.set_tool(Tool::Pencil);
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "切工具视为 ×"
    );
}
