use super::commit_pending_drag_and_drain;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== commit_pending_drag 行为 =====

#[test]
fn test_commit_pending_drag_when_none_returns_false() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    assert!(!commit_pending_drag_and_drain(&mut editor));
    assert!(editor.pending_drag_state.is_none());
}

#[test]
fn test_commit_pending_drag_with_zero_delta_returns_false_and_clears() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    // 直接设置一个 delta 为零的 pending
    let zero_drag = DragState::from_single(0, 1, 0, 60); // delta = (0, 0)
    editor.pending_drag_state = Some(zero_drag);

    assert!(
        !commit_pending_drag_and_drain(&mut editor),
        "delta 零应返回 false"
    );
    assert!(editor.pending_drag_state.is_none(), "pending 应被清空");
    // notes 未被修改
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    );
}

#[test]
fn test_commit_pending_drag_applies_delta_to_notes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 选中索引 0，delta=(200, 7)
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    editor.pending_drag_state = Some(drag);

    assert!(commit_pending_drag_and_drain(&mut editor));
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        200.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        67
    );
    // note 1 未选中，不变
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        240.0
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        62
    );
    // pending 已清空
    assert!(editor.pending_drag_state.is_none());
}

#[test]
fn test_commit_pending_drag_clamps_negative_tick_to_zero() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(50.0, 60, 480.0)]);

    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(-200, 0); // 50 - 200 = -150，应 clamp 到 0
    editor.pending_drag_state = Some(drag);

    assert!(commit_pending_drag_and_drain(&mut editor));
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0,
        "应 clamp 到 0"
    );
}

// ===== is_editing / has_pending_drag 在 pending 状态 =====

#[test]
fn test_is_editing_returns_true_when_pending_drag_exists() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.pending_drag_state = Some(DragState::from_single(0, 1, 0, 60));
    assert!(
        editor.is_editing(),
        "pending 状态应视为编辑中（拦截 Undo/Save）"
    );
    assert!(editor.has_pending_drag());
}

#[test]
fn test_undo_blocked_when_pending_drag_exists() {
    // 用户选择"拦截并 Toast 提示"策略：pending 状态下 Undo 被拦截
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.push_history();
    // 2026-08 单一权威源：直接修改 document 当前轨（track_notes_mut）
    if let Some(track) = editor
        .editor_state
        .data
        .document
        .as_mut()
        .and_then(|doc| doc.track_notes_mut(0))
        && let Some(note) = track.get_mut(0)
    {
        note.start_tick = 100;
        note.end_tick = note.end_tick.max(note.start_tick + 1);
    }

    // 现在 pending_drag_state 存在（模拟用户拖动后未点击空白处）
    editor.pending_drag_state = Some(DragState::from_single(0, 1, 0, 60));
    assert!(!editor.undo(), "pending 状态下 Undo 应被拦截");
}
