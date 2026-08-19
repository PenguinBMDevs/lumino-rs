use super::setup_dragging;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

// ===== 单音符拖动完整流程 =====

#[test]
fn test_single_note_drag_commit_undo_redo_flow() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 拖动前：tick=0, key=60
    setup_dragging(&mut editor, 0, 100, 5);

    // 松手提交
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

    // 撤销：恢复原位置
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

    // 重做：再次应用移动
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
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        65
    );
}

#[test]
fn test_single_note_drag_with_clamp_undo_restores_original() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(50.0, 100, 480.0)]);

    // 拖动到 key=200（超过 max_key=127，应 clamp）
    setup_dragging(&mut editor, 0, 0, 100);

    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        127,
        "应 clamp 到 127"
    );

    // 撤销应恢复到 key=100（原值），而不是 clamp 前的 200
    assert!(editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        100
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        50.0
    );
}
