use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== 移动 + 复制并存（提交顺序正确性） =====

#[test]
fn test_move_and_copy_both_pending_commit_in_correct_order() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 场景：先移动选中音符（松手 → pending_drag），再 Ctrl 复制同一选区（松手 → pending_copy）
    // 移动音符 A（索引 0）→ delta=(100, 0)
    let mut move_drag = DragState::from_indices([0], 2, 0, 60);
    move_drag.set_delta(100, 0);
    editor.pending_drag_state = Some(move_drag);
    // 复制音符 B（索引 1）→ delta=(50, 0)：副本 tick=290 追加在尾部
    let mut copy_drag = DragState::from_indices([1], 2, 0, 60);
    copy_drag.set_delta(50, 0);
    editor.pending_copy_drag_state = Some(copy_drag);

    // 点击空白处：flush → 先 drain 移动异步提交，再提交复制
    editor.handle_tool_pressed(iced_core::Point::new(9999.0, 9999.0), false, 9999.0, 0);

    let data = &editor.editor_state.data;
    // 音符 A 已移动（tick 0 → 100），B 保持（240），副本 B'（290）已写入
    assert_eq!(data.current_track_note_count(), 3);
    // 排序后: [A(100), B(240), B'(290)]
    assert_eq!(
        data.get_note_view(0).expect("note should exist").tick,
        100.0,
        "A 应被移动"
    );
    assert_eq!(data.get_note_view(0).expect("note should exist").key, 60);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        240.0,
        "B 不变"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        290.0,
        "B 副本已写入"
    );
    assert_eq!(data.get_note_view(2).expect("note should exist").key, 62);
    assert!(editor.pending_drag_state.is_none());
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_commit_current_edit_with_move_and_copy_keeps_both() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 移动 A（索引 0）delta=(100,0) + 复制 B（索引 1）delta=(50,0)
    let mut move_drag = DragState::from_indices([0], 2, 0, 60);
    move_drag.set_delta(100, 0);
    editor.pending_drag_state = Some(move_drag);
    let mut copy_drag = DragState::from_indices([1], 2, 0, 60);
    copy_drag.set_delta(50, 0);
    editor.pending_copy_drag_state = Some(copy_drag);

    // Save/Play/Export 前的自动提交：drain 移动后写副本
    assert!(editor.commit_current_edit());

    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 3);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").tick,
        100.0,
        "A 应被移动"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        290.0,
        "B 副本已写入"
    );
    assert!(editor.pending_drag_state.is_none());
    assert!(editor.pending_copy_drag_state.is_none());
}
