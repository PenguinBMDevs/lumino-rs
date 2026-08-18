use super::setup_dragging;
use super::setup_dragging_selection;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

// ===== 批量拖动完整流程 =====

#[test]
fn test_batch_drag_commit_undo_redo_flow() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 62, 240.0),
            Note::new(480.0, 64, 240.0),
        ],
    );

    // 批量选中索引 0 和 2，拖动 +200, +7
    setup_dragging_selection(&mut editor, [0, 2], 200, 7);

    assert!(editor.commit_current_edit());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        200.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        67
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        240.0
    ); // 未选中，不变
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        62
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        680.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        71
    );

    // 撤销：所有选中音符恢复原位置
    assert!(editor.undo());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        480.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // 重做
    assert!(editor.redo());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        200.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        680.0
    );
}

// ===== 连续多次拖动：每次拖动是独立的 undo 节点 =====

#[test]
fn test_multiple_drags_create_independent_undo_steps() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 第一次拖动：+100, +5
    setup_dragging(&mut editor, 0, 100, 5);
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

    // 第二次拖动：再 +50, +3
    setup_dragging(&mut editor, 0, 50, 3);
    assert!(editor.commit_current_edit());
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
    );

    // 第一次撤销：回退第二次拖动
    assert!(editor.undo());
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

    // 第二次撤销：回退第一次拖动
    assert!(editor.undo());
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

    // 无法继续撤销
    assert!(!editor.can_undo());

    // 两次重做
    assert!(editor.redo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert!(editor.redo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        150.0
    );
}
