use super::commit_pending_drag_and_drain;
use super::start_dragging_selection;
use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== 延迟提交流程：DraggingSelection 松手 → pending → commit =====

#[test]
fn test_dragging_selection_release_saves_to_pending_not_notes() {
    // 松手时不 apply 到 notes，只保存到 pending_drag_state
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    start_dragging_selection(&mut editor, [0], 100, 5);
    // 松手
    editor.handle_released();

    // notes 未被修改（延迟提交）
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
    // pending 已保存
    assert!(editor.has_pending_drag());
    let pending = editor
        .pending_drag_state
        .as_ref()
        .expect("pending_drag_state 应已保存");
    assert_eq!(pending.delta_tick, 100);
    assert_eq!(pending.delta_key, 5);
    // edit_state 已回到 Idle
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));
}

#[test]
fn test_dragging_selection_zero_delta_not_saved_to_pending() {
    // delta 为零时不保存 pending
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    start_dragging_selection(&mut editor, [0], 0, 0); // delta 零
    editor.handle_released();

    assert!(!editor.has_pending_drag(), "delta 零不应保存 pending");
}

// ===== commit_current_edit 提交 pending =====

#[test]
fn test_commit_current_edit_commits_pending_drag() {
    // pending 状态下调用 commit_current_edit 应提交 pending（Save/Play/Export 前的 fallback）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    start_dragging_selection(&mut editor, [0], 100, 5);
    editor.handle_released();
    assert!(editor.has_pending_drag());

    // 模拟用户按 Save：commit_current_edit 应提交 pending
    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        65
    );
    assert!(!editor.has_pending_drag(), "commit 后 pending 应清空");
    assert!(!editor.is_editing(), "commit 后应退出编辑状态");
}

#[test]
fn test_commit_current_edit_when_pending_only_returns_true() {
    // pending 状态下（edit_state=Idle）commit_current_edit 也应触发提交
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 直接设置 pending，不进入 DraggingSelection（模拟松手后的状态）
    let mut drag = DragState::from_single(0, 1, 0, 60);
    drag.set_delta(80, 4);
    editor.pending_drag_state = Some(drag);
    // edit_state 是 Idle
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));

    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        80.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        64
    );
    assert!(!editor.has_pending_drag());
}

// ===== flush_pending_drag（点击空白处提交）行为验证 =====

#[test]
fn test_idle_with_pending_then_commit_clears_pending() {
    // 模拟用户拖动后松手（pending 状态）→ 点击空白处提交
    // flush_pending_drag 是 private，通过 commit_pending_drag 验证
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(100.0, 62, 240.0)],
    );

    start_dragging_selection(&mut editor, [0, 1], 50, 2);
    editor.handle_released();

    // 模拟点击空白处：commit_pending_drag
    assert!(commit_pending_drag_and_drain(&mut editor));
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        50.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        62
    ); // 60 + 2
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        150.0
    ); // 100 + 50
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        64
    ); // 62 + 2
    assert!(!editor.has_pending_drag());
}
