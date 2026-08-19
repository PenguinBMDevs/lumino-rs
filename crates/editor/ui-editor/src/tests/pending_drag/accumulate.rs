use super::commit_pending_drag_and_drain;
use super::start_dragging_selection;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

// ===== 累积 delta：多次拖动叠加 =====

#[test]
fn test_accumulated_delta_two_drags_sum_up() {
    // 两次拖动同一选区：第一次 delta=(100,5)，第二次 delta=(50,3)
    // 累积后 pending delta 应为 (150,8)
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 第一次拖动
    start_dragging_selection(&mut editor, [0], 100, 5);
    editor.handle_released();
    assert!(editor.has_pending_drag());
    assert_eq!(
        editor
            .pending_drag_state
            .as_ref()
            .expect("pending 应存在")
            .delta_tick,
        100
    );
    assert_eq!(
        editor
            .pending_drag_state
            .as_ref()
            .expect("pending 应存在")
            .delta_key,
        5
    );

    // 第二次拖动（累积模式：不重复 push_history）
    start_dragging_selection(&mut editor, [0], 50, 3);
    editor.handle_released();

    // 累积 delta = (100+50, 5+3) = (150, 8)
    assert!(editor.has_pending_drag());
    let pending = editor
        .pending_drag_state
        .as_ref()
        .expect("累积后 pending 应存在");
    assert_eq!(pending.delta_tick, 150, "delta_tick 应累积");
    assert_eq!(pending.delta_key, 8, "delta_key 应累积");

    // notes 仍未被修改（延迟提交）
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

    // 提交累积 delta
    assert!(commit_pending_drag_and_drain(&mut editor));
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        150.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        68
    ); // 60 + 8
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_accumulated_delta_three_drags_with_negative() {
    // 三次拖动：(+100,+5) → (+50,-3) → (-200,0)
    // 累积 = (-50, 2)
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 480.0)]);

    start_dragging_selection(&mut editor, [0], 100, 5);
    editor.handle_released();

    start_dragging_selection(&mut editor, [0], 50, -3);
    editor.handle_released();

    start_dragging_selection(&mut editor, [0], -200, 0);
    editor.handle_released();

    let pending = editor
        .pending_drag_state
        .as_ref()
        .expect("三次累积后 pending 应存在");
    assert_eq!(pending.delta_tick, -50, "100+50-200 = -50");
    assert_eq!(pending.delta_key, 2, "5-3+0 = 2");

    // 提交：100 + (-50) = 50
    assert!(commit_pending_drag_and_drain(&mut editor));
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        50.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        62
    ); // 60 + 2
}

#[test]
fn test_accumulated_delta_only_one_history_push() {
    // 累积模式下，多次拖动只 push 一次 history（一次逻辑操作一条记录）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    let history_len_before = editor.editor_state.data.history.undo_len();

    start_dragging_selection(&mut editor, [0], 100, 5);
    editor.handle_released();

    let history_len_after_first = editor.editor_state.data.history.undo_len();
    assert_eq!(
        history_len_after_first,
        history_len_before + 1,
        "首次拖动应 push 一次 history"
    );

    start_dragging_selection(&mut editor, [0], 50, 3);
    editor.handle_released();

    let history_len_after_second = editor.editor_state.data.history.undo_len();
    assert_eq!(
        history_len_after_second, history_len_after_first,
        "累积模式下第二次拖动不应 push history"
    );
}
