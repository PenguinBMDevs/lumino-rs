//! 画笔工具单测 — 撤销/重做历史（从 tests.rs 拆出）

use super::*;

#[test]
fn test_pending_strokes_undo_redo_one_by_one() {
    let mut editor = brush_editor();
    for key in [VISIBLE_KEY, VISIBLE_KEY + 4.0, VISIBLE_KEY + 8.0] {
        stroke_via_actions(&mut editor, &[(0.0, key), (2400.0, key)]);
    }
    assert_eq!(editor.editor_state.brush_tool.strokes.len(), 3);
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_some(),
        "待确认笔画应显示 √× 按钮"
    );

    // 逐笔撤销：每次只掉一笔
    for expected in [2usize, 1, 0] {
        editor.handle_action(EditorAction::Undo);
        assert_eq!(
            editor.editor_state.brush_tool.strokes.len(),
            expected,
            "Ctrl+Z 必须逐笔撤销"
        );
    }
    assert!(
        !editor.can_undo(),
        "笔画撤空后无更多可撤销（document 历史亦为空）"
    );
    assert!(
        crate::grid::brush_tool_box::brush_button_rects(&editor).is_none(),
        "笔画撤空后按钮消失"
    );

    // 逐笔重做：每次只回一笔
    for expected in [1usize, 2, 3] {
        editor.handle_action(EditorAction::Redo);
        assert_eq!(
            editor.editor_state.brush_tool.strokes.len(),
            expected,
            "Ctrl+Y 必须逐笔重做"
        );
    }
    assert!(!editor.can_redo());
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "笔画期操作不得写入 document 历史"
    );
}

#[test]
fn test_drag_stroke_is_one_undo_step() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);
    let before = editor.editor_state.brush_tool.strokes[0].points.clone();

    // 按住笔画实心区拖动（X 自由、Y 单 key 吸附）
    let body = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY));
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(body.x, body.y),
        shift: false,
        ctrl: false,
    });
    let target = editor.line_pos_screen_pos((1337.0, VISIBLE_KEY + 2.4));
    editor.handle_action(EditorAction::Moved(Point2::new(target.x, target.y)));
    editor.handle_action(EditorAction::Released);

    let moved = editor.editor_state.brush_tool.strokes[0].points.clone();
    assert_ne!(moved, before, "拖动应改变笔画位置");
    assert_eq!(moved[0].1, VISIBLE_KEY + 2.0, "Y 向按单个 key 吸附");

    // 一次 Ctrl+Z 回到拖动前；一次 Ctrl+Y 回到拖动后
    editor.handle_action(EditorAction::Undo);
    assert_eq!(
        editor.editor_state.brush_tool.strokes[0].points, before,
        "整段拖动 = 一步撤销"
    );
    editor.handle_action(EditorAction::Redo);
    assert_eq!(editor.editor_state.brush_tool.strokes[0].points, moved);
}

#[test]
fn test_noop_drag_adds_no_dead_undo_step() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);

    // 按住笔画实心区但不移动 → 不应产生"按了没反应"的空撤销步
    let body = editor.line_pos_screen_pos((1200.0, VISIBLE_KEY));
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(body.x, body.y),
        shift: false,
        ctrl: false,
    });
    editor.handle_action(EditorAction::Released);

    editor.handle_action(EditorAction::Undo);
    assert!(
        editor.editor_state.brush_tool.strokes.is_empty(),
        "空拖动不得占用撤销步（一次 Ctrl+Z 应直接撤销该笔）"
    );
}

#[test]
fn test_cancel_discards_stroke_history_too() {
    let mut editor = brush_editor();
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY)]);
    stroke_via_actions(&mut editor, &[(480.0, VISIBLE_KEY + 2.0)]);
    assert!(editor.can_undo(), "有待确认笔画即有笔画历史");

    editor.handle_action(EditorAction::BrushCancel);
    assert!(!editor.editor_state.brush_tool.has_pending());
    assert!(
        !editor.can_undo() && !editor.can_redo(),
        "× 取消后笔画历史栈一并清空"
    );
    assert!(
        !editor.editor_state.data.history.can_undo(),
        "× 不产生 document 历史"
    );
}

#[test]
fn test_confirm_then_undo_removes_all_notes_and_strokes_stay_gone() {
    let mut editor = brush_editor();
    editor.brush.set_thickness(2);
    stroke_via_actions(&mut editor, &[(0.0, VISIBLE_KEY), (2400.0, VISIBLE_KEY)]);
    stroke_via_actions(
        &mut editor,
        &[(0.0, VISIBLE_KEY + 2.0), (2400.0, VISIBLE_KEY + 2.0)],
    );

    editor.handle_action(EditorAction::BrushConfirm);
    let generated = notes_of(&editor, 1).len();
    assert!(generated > 0, "√ 应生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "√ 后清空笔画"
    );
    assert!(
        !editor.editor_state.brush_tool.can_undo_path(),
        "√ 后笔画历史栈清空（document 历史单独存在）"
    );
    assert!(
        editor.editor_state.data.history.can_undo(),
        "√ 写入 document 产生一条记录"
    );

    // Ctrl+Z：一次回退全部生成音符（卡片要求），且笔画**不会**被还原
    editor.handle_action(EditorAction::Undo);
    assert!(notes_of(&editor, 1).is_empty(), "一次撤销回退全部生成音符");
    assert!(
        !editor.editor_state.brush_tool.has_pending(),
        "√ 后撤销不得把笔画还原成待确认状态"
    );

    // Ctrl+Y：音符回来
    editor.handle_action(EditorAction::Redo);
    assert_eq!(notes_of(&editor, 1).len(), generated, "Ctrl+Y 恢复生成音符");
}
