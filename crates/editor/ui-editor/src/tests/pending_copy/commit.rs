use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== commit_pending_copy 行为 =====

#[test]
fn test_commit_pending_copy_when_none_returns_false() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    assert!(!editor.commit_pending_copy());
    // 音符数不变
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
}

#[test]
fn test_commit_pending_copy_zero_delta_returns_false_and_clears() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    // delta 为零的 pending 复制（未拖动，直接松手）
    let zero_drag = DragState::from_single(0, 1, 0, 60);
    editor.pending_copy_drag_state = Some(zero_drag);

    assert!(
        !editor.commit_pending_copy(),
        "delta 零应返回 false（未产生副本）"
    );
    assert!(editor.pending_copy_drag_state.is_none(), "pending 应被清空");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "不应插入任何音符"
    );
}

#[test]
fn test_commit_pending_copy_inserts_copies_and_selects_them() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 选中索引 0，delta=(200, 7)：复制出一个偏移副本
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    // 原始 2 个音符保留，副本插入：2 → 3 个音符
    assert_eq!(data.current_track_note_count(), 3);
    // batch_insert 按 start_tick 有序插入：副本 tick=200 落在 0 与 240 之间 → 索引 1
    assert_eq!(data.get_note_view(0).expect("note should exist").tick, 0.0);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").key,
        60,
        "原始音符不变"
    );
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        200.0
    );
    assert_eq!(data.get_note_view(1).expect("note should exist").key, 67);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").length,
        480.0,
        "长度复制自原始"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        240.0
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").key,
        62,
        "未选中音符不变"
    );
    // 副本被选中（按参数精确匹配，不误选未选中的原始音符）
    assert!(editor.editor_state.interaction.selected_notes.contains(&1));
    assert!(
        !editor.editor_state.interaction.selected_notes.contains(&0),
        "原件不应保持选中（只保留最新件框选，用户要求）"
    );
    assert!(!editor.editor_state.interaction.selected_notes.contains(&2));
    // pending 已清空
    assert!(editor.pending_copy_drag_state.is_none());
}

#[test]
fn test_commit_pending_copy_multiple_notes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(240.0, 62, 240.0),
            Note::new(480.0, 64, 120.0),
        ],
    );

    // 全选复制：delta=(100, 3)
    let mut drag = DragState::from_indices([0, 1, 2], 3, 0, 60);
    drag.set_delta(100, 3);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 6);
    // 有序插入后交错排列：原始 0/240/480 与副本 100/340/580
    // 0: 原始0    1: 副本100   2: 原始240   3: 副本340   4: 原始480   5: 副本580
    assert_eq!(data.get_note_view(0).expect("note should exist").tick, 0.0);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        100.0
    );
    assert_eq!(data.get_note_view(1).expect("note should exist").key, 63);
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        240.0
    );
    assert_eq!(
        data.get_note_view(3).expect("note should exist").tick,
        340.0
    );
    assert_eq!(data.get_note_view(3).expect("note should exist").key, 65);
    assert_eq!(
        data.get_note_view(4).expect("note should exist").tick,
        480.0
    );
    assert_eq!(
        data.get_note_view(5).expect("note should exist").tick,
        580.0
    );
    assert_eq!(data.get_note_view(5).expect("note should exist").key, 67);
    // 副本（散布索引 1/3/5）选中；原件（0/2/4）不再框选（只保留最新件框选）
    for i in [1usize, 3, 5] {
        assert!(
            editor.editor_state.interaction.selected_notes.contains(&i),
            "副本索引 {} 应被选中",
            i
        );
    }
    for i in [0usize, 2, 4] {
        assert!(
            !editor.editor_state.interaction.selected_notes.contains(&i),
            "原件索引 {} 不应保持选中（只保留最新件框选）",
            i
        );
    }
}

#[test]
fn test_commit_pending_copy_clamps_key_range() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // key 下移 100：60 - 100 = -40 → clamp 到 0
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(0, -100);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 2);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").key,
        60,
        "原始音符不变"
    );
    assert_eq!(
        data.get_note_view(1).expect("note should exist").key,
        0,
        "副本 key 应 clamp 到 0"
    );
}
