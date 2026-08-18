use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== handle_pointer_pressed Ctrl + 选择框内部 → 复制拖拽 =====

/// 计算选中音符选择框的屏幕中心点（保证 Inside 命中）
fn selection_box_center(editor: &Editor) -> iced_core::Point {
    let (x1, x2, y1, y2) = editor
        .get_selection_box_bounds()
        .expect("有选中音符时应能计算选择框");
    iced_core::Point::new((x1 + x2) / 2.0, (y1 + y2) / 2.0)
}

#[test]
fn test_pointer_pressed_ctrl_inside_enters_copy_state() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);
    editor.set_ctrl_pressed(true);

    // 点击选择框内部 → Ctrl 按下 → DraggingSelectionCopy
    let center = selection_box_center(&editor);
    editor.handle_tool_pressed(center, false, 0.0, 60);

    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelectionCopy { .. } => {}
        other => panic!("Ctrl 拖动应进入 DraggingSelectionCopy，实际 {:?}", other),
    }
}

#[test]
fn test_pointer_pressed_without_ctrl_enters_move_state() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);
    editor.set_ctrl_pressed(false);

    // 点击选择框内部 → 未按 Ctrl → DraggingSelection（移动）
    let center = selection_box_center(&editor);
    editor.handle_tool_pressed(center, false, 0.0, 60);

    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelection { .. } => {}
        other => panic!("非 Ctrl 拖动应进入 DraggingSelection，实际 {:?}", other),
    }
}

// ===== 状态判定 / 自动提交 =====

#[test]
fn test_is_editing_with_pending_copy_returns_true() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.pending_copy_drag_state = Some(DragState::from_single(0, 1, 0, 60));
    assert!(editor.is_editing(), "pending copy 应视为编辑状态");
    assert!(editor.has_pending_drag());
}

#[test]
fn test_commit_current_edit_commits_pending_copy() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 模拟：复制拖动松手后（pending_copy 存在），用户直接触发 Save/Play/Export
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_current_edit());
    // 副本已写入内存层
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(1)
            .expect("note should exist")
            .tick,
        100.0
    );
    assert!(editor.pending_copy_drag_state.is_none());
}

#[test]
fn test_flush_pending_drag_commits_copy_on_empty_click() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 复制松手后 pending_copy 存在
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 模拟点击空白处（flush_pending_drag 由 handle_pointer_pressed 空白分支调用）
    // 直接调用私有路径不可达，改用公开入口：点击空白会先 flush 再开始新框选
    editor.handle_tool_pressed(iced_core::Point::new(9999.0, 9999.0), false, 9999.0, 0);

    // 副本已写入内存层（点击空白处退出框选状态 → 写入）
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(1)
            .expect("note should exist")
            .tick,
        100.0
    );
    assert!(
        editor.pending_copy_drag_state.is_none(),
        "复制提交后 pending 应清空"
    );
}
