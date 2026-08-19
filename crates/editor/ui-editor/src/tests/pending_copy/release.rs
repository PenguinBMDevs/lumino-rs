use super::release_copy_drag;
use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== handle_released 松手行为（松手即提交） =====

#[test]
fn test_released_copy_drag_commits_immediately() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );
    // 预选中索引 0（进入 DraggingSelectionCopy 的前提：已有选区）
    editor.selection_insert(0);

    // 复制拖动中（Ctrl+拖动）：原始音符不动，仅维护 delta
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    release_copy_drag(&mut editor, drag);

    // 松手后：**立即写入 document**（松手即提交，副本真实化）
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        3,
        "副本应已写入内存"
    );
    // 原件不变
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("note should exist")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("note should exist")
            .key,
        60
    );
    // 副本在偏移位置（tick 200, key 67）
    let copy = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .find(|n| n.start_tick == 200)
        .expect("副本应存在");
    assert_eq!(copy.key, 67);
    // pending 已清空（提交完成）
    assert!(editor.pending_copy_drag_state.is_none());
    // 副本被选中（最新件框选）
    assert!(editor.has_selection());
    // 编辑状态回到 Idle
    assert_eq!(editor.editor_state.interaction.edit_state, EditState::Idle);
}

#[test]
fn test_released_copy_drag_zero_delta_does_not_save() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 未拖动（delta 零）直接松手 → 不产生副本
    let zero_drag = DragState::from_single(0, 1, 0, 60);
    release_copy_drag(&mut editor, zero_drag);

    assert!(editor.pending_copy_drag_state.is_none());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
}

#[test]
fn test_released_copy_drag_selects_copies() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );
    // 预选中索引 0、1
    editor.selection_insert(0);
    editor.selection_insert(1);

    let mut drag = DragState::from_indices([0, 1], 2, 0, 60);
    drag.set_delta(50, 0);
    release_copy_drag(&mut editor, drag);

    // 松手即提交：document 4 个音符（原件 + 副本），副本选中
    assert_eq!(editor.editor_state.data.current_track_note_count(), 4);
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(editor.has_selection(), "提交后副本应保持选中（最新件框选）");
    // 只选中副本（tick 50 / 290），原件不选中
    let selected: Vec<usize> = editor.get_selected_indices();
    let mut ticks: Vec<f32> = selected
        .iter()
        .filter_map(|&i| editor.editor_state.data.get_note_view(i))
        .map(|n| n.tick)
        .collect();
    ticks.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(ticks, vec![50.0, 290.0], "应只选中副本");
}
