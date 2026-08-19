use super::setup_dragging;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_note_core::history::OpKind;

// ===== 零 delta 拖动：不产生实际变更 =====

#[test]
fn test_zero_delta_drag_commit_undo_is_noop() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 零 delta 拖动
    setup_dragging(&mut editor, 0, 0, 0);

    // commit 仍然返回 true（is_editing=true），但 notes 不变
    assert!(editor.commit_current_edit());
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

    // 零 delta 不生成 MoveOp，也不 push 快照，因此没有可撤销的操作
    assert!(!editor.can_undo(), "零 delta 拖动不应产生历史记录");
    assert!(!editor.undo());
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

// ===== 跨 group 逻辑撤销（直接调用 data.undo_logical）=====
//
// 此测试验证：当单个逻辑操作因超过 entry_limit 被分割为多个 group 时，
// `undo_logical` 能一次性回退整个逻辑操作链。

#[test]
fn test_cross_group_logical_undo_after_split() {
    let mut editor = Editor::new();

    // 2026-08 单一权威源：空 document 种子（后续 insert_note 写入当前轨）
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    // 将 entry_limit 设为 3，merge_window=300ms，使得 4 次连续 NoteCreate 会触发分割
    // 注意：merge_window=0 表示不合并，必须 >0 才能触发合并/分割逻辑
    editor.editor_state.data.history.set_config(100, 300, 3);

    // 初始状态：0 个音符
    // 连续放置 4 个 NoteCreate（在合并窗口内）
    // **关键**：push_history_mergeable 必须在修改 notes 之前调用（与 finish_drawing 一致），
    // 这样快照捕获的是"操作前"状态，undo 时才能正确恢复
    for i in 0..4u16 {
        let _ = editor
            .editor_state
            .data
            .push_history_mergeable(OpKind::NoteCreate);
        editor.editor_state.data.insert_note(
            editor.editor_state.data.current_track,
            Note::new(i as f32 * 100.0, 60 + i, 80.0),
        );
    }

    // 现在有 4 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 4);
    // undo_stack 应有 2 个 group：group 1（3 entries）+ group 2（1 entry，parent=1）
    assert_eq!(editor.editor_state.data.history.undo_len(), 2);

    // 标准 undo 只回退一步（1 个音符）——验证分割确实发生了
    assert!(editor.editor_state.data.undo());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 3);

    // 此时 undo_stack 只剩 group 1（3 entries，notes=[]）
    assert_eq!(editor.editor_state.data.history.undo_len(), 1);

    // 逻辑撤销：group 1 的快照 notes=[]（chain 开始前的状态），
    // undo_logical 应一次性回退整个 chain，恢复到 0 个音符
    assert!(editor.editor_state.data.undo_logical());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "逻辑撤销应回退剩余 NoteCreate chain，恢复到 0 个音符"
    );
}
