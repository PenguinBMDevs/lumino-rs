//! 主选择漂移防护回归测试
//!
//! 背景：主选择（`selected_notes`）以**当前轨索引**存储；任何结构编辑
//! （远端增删移、undo/redo 回放、razor 切割、拖动写回、绘制插入）位移索引后，
//! 选中集会指向错误音符（用户报告：A 端选中一段后 B 端在同轨头部插入音符，
//! A 端再按 Delete 会命中错误音符）。修复：`SelectionIdentity` guard
//! （编辑前按 id 捕获、编辑后重映射）。
//!
//! 断言统一走「选中音符的 id 列表」而非裸索引——id 是跨结构编辑的稳定身份，
//! 避免测试与实现同源漂移。

use crate::tests::test_helpers;
use crate::{Editor, Note};
use lumino_editor_state::DragState;

/// 当前选中音符的 id 列表（升序）：漂移防护的稳定断言口径
fn selected_ids(editor: &Editor) -> Vec<u64> {
    let track = editor.editor_state.data.current_track;
    let notes = editor.editor_state.data.track_notes(track);
    let mut ids: Vec<u64> = editor
        .get_selected_indices()
        .into_iter()
        .filter_map(|i| notes.get(i))
        .map(|ev| ev.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// 当前选中索引（升序）
fn selected_indices(editor: &Editor) -> Vec<usize> {
    let mut v = editor.get_selected_indices();
    v.sort_unstable();
    v
}

/// 当前轨第 `idx` 个音符的 id
fn note_id_at(editor: &Editor, idx: usize) -> u64 {
    editor
        .editor_state
        .data
        .current_track_notes()
        .get(idx)
        .expect("音符应存在")
        .id
}

/// 种子：轨 1（当前轨）3 个音符（tick 0/480/960），返回其 id
fn seed_three(editor: &mut Editor) -> (u64, u64, u64) {
    let notes = vec![
        Note::from_raw(0.0, 60, 240.0, 100, 0),
        Note::from_raw(480.0, 62, 240.0, 100, 0),
        Note::from_raw(960.0, 64, 240.0, 100, 0),
    ];
    test_helpers::seed_notes(editor, 2, 1, &notes);
    (
        note_id_at(editor, 0),
        note_id_at(editor, 1),
        note_id_at(editor, 2),
    )
}

#[test]
fn test_remap_follows_insert_shift() {
    let mut editor = Editor::default();
    let (_, b_id, c_id) = seed_three(&mut editor);
    // 选中 B、C（索引 1、2）
    editor.selection_insert(1);
    editor.selection_insert(2);

    let identity = editor.capture_selection_identity();
    // 头部插入新音符（tick 0，稳定插到 A 之后）→ B/C 索引右移 1
    editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(0.0, 50, 120.0, 100, 0));
    editor.remap_selection_by_identity(&identity);

    assert_eq!(
        selected_ids(&editor),
        vec![b_id, c_id],
        "插入后选中应仍为 B、C"
    );
    assert_eq!(selected_indices(&editor), vec![2, 3], "索引应随插入右移");
}

#[test]
fn test_remap_deselects_deleted_note() {
    let mut editor = Editor::default();
    let (a_id, _, c_id) = seed_three(&mut editor);
    // 选中 A、C（索引 0、2）
    editor.selection_insert(0);
    editor.selection_insert(2);

    let identity = editor.capture_selection_identity();
    // 删除 B（索引 1）→ C 左移
    editor.editor_state.data.remove_note(1, 1);
    editor.remap_selection_by_identity(&identity);

    assert_eq!(
        selected_ids(&editor),
        vec![a_id, c_id],
        "被删音符应取消选中"
    );
    assert_eq!(selected_indices(&editor), vec![0, 1], "索引应随删除左移");
}

#[test]
fn test_remap_deselects_deleted_selected_note() {
    let mut editor = Editor::default();
    let (a_id, b_id, c_id) = seed_three(&mut editor);
    // 全选
    editor.selection_insert(0);
    editor.selection_insert(1);
    editor.selection_insert(2);

    let identity = editor.capture_selection_identity();
    // 删除被选中的 B（索引 1）
    editor.editor_state.data.remove_note(1, 1);
    editor.remap_selection_by_identity(&identity);

    assert_eq!(
        selected_ids(&editor),
        vec![a_id, c_id],
        "被删的选中音符应移除，其余保留"
    );
    assert_eq!(selected_indices(&editor), vec![0, 1]);
    assert_ne!(b_id, 0, "B 的 id 非零（种子不变量）");
}

#[test]
fn test_remap_follows_moved_note() {
    let mut editor = Editor::default();
    let (a_id, _, _) = seed_three(&mut editor);
    editor.selection_insert(0); // 选中 A

    let identity = editor.capture_selection_identity();
    // A 从 tick 0 移到 tick 2000（落末尾：B、C 左移，A 到索引 2）
    editor
        .editor_state
        .data
        .update_note(1, 0, Note::from_raw(2000.0, 60, 240.0, 100, 0));
    editor.remap_selection_by_identity(&identity);

    assert_eq!(selected_ids(&editor), vec![a_id], "选中应跟随移动的音符");
    assert_eq!(selected_indices(&editor), vec![2], "A 应落在新位置索引 2");
}

#[test]
fn test_remap_noop_when_track_switched() {
    let mut editor = Editor::default();
    let (_, b_id, _) = seed_three(&mut editor);
    editor.selection_insert(1); // 选中 B

    let identity = editor.capture_selection_identity();
    // 捕获后切换当前轨（生产路径会清空选择；此处直接改索引验证守卫早退）
    editor.editor_state.data.current_track = 0;
    editor.remap_selection_by_identity(&identity);

    assert_eq!(
        editor.get_selected_indices(),
        vec![1],
        "捕获轨已非当前轨：不得按旧轨身份重映射"
    );
    assert_ne!(b_id, 0);
}

#[test]
fn test_undo_redo_preserves_selection_identity() {
    let mut editor = Editor::default();
    let (_, b_id, _) = seed_three(&mut editor);
    editor.selection_insert(1); // 选中 B（索引 1）

    // 模拟一次守卫操作：头部插入新音符（B 右移），guard 保持选中为 B
    let identity = editor.capture_selection_identity();
    editor.push_history();
    editor
        .editor_state
        .data
        .insert_note(1, Note::from_raw(0.0, 50, 120.0, 100, 0));
    editor.remap_selection_by_identity(&identity);
    assert_eq!(selected_ids(&editor), vec![b_id], "插入后选中应仍为 B");
    assert_eq!(selected_indices(&editor), vec![2]);

    // 撤销：轨道恢复，B 回到索引 1；选中应仍为 B（而非漂移到其它音符）
    assert!(editor.undo());
    assert_eq!(selected_ids(&editor), vec![b_id], "undo 后选中不得漂移");
    assert_eq!(selected_indices(&editor), vec![1], "B 回到原索引");

    // 重做：B 再次右移到索引 2；选中应仍为 B
    assert!(editor.redo());
    assert_eq!(selected_ids(&editor), vec![b_id], "redo 后选中不得漂移");
    assert_eq!(selected_indices(&editor), vec![2]);
}

#[test]
fn test_razor_split_preserves_selection() {
    let mut editor = Editor::default();
    // doc 轨 2 放 3 个音符；视觉序 [2,0,1] → 视觉 0 = doc 轨 2
    let notes = vec![
        Note::from_raw(0.0, 60, 480.0, 100, 0),
        Note::from_raw(480.0, 62, 480.0, 100, 0),
        Note::from_raw(960.0, 64, 480.0, 100, 0),
    ];
    test_helpers::seed_notes(&mut editor, 3, 2, &notes);
    editor.editor_state.data.track_visual_order = vec![2, 0, 1];
    let b_id = note_id_at(&editor, 1);
    let c_id = note_id_at(&editor, 2);
    // 选中 B、C（索引 1、2）
    editor.selection_insert(1);
    editor.selection_insert(2);

    // 切割 A（0..480，含 tick 240）→ A 变两个音符，B/C 右移 1
    let split = editor.arrange_razor(240.0, 0);
    assert_eq!(split, 1, "应切割 1 个音符");
    assert_eq!(
        selected_ids(&editor),
        vec![b_id, c_id],
        "razor 后选中不得漂移"
    );
    assert_eq!(selected_indices(&editor), vec![2, 3], "B/C 应右移 1");
}

#[test]
fn test_finish_drawing_preserves_selection() {
    let mut editor = Editor::default();
    // 音符从 tick 480 起，便于在 tick 0 绘制（落在所有音符之前）
    let notes = vec![
        Note::from_raw(480.0, 62, 240.0, 100, 0),
        Note::from_raw(960.0, 64, 240.0, 100, 0),
        Note::from_raw(1440.0, 66, 240.0, 100, 0),
    ];
    test_helpers::seed_notes(&mut editor, 2, 1, &notes);
    let b_id = note_id_at(&editor, 0);
    let c_id = note_id_at(&editor, 1);
    editor.selection_insert(0);
    editor.selection_insert(1);

    // 在 tick 0 绘制新音符（落在选中音符之前 → 索引右移 1）
    editor.finish_drawing(0.0, 50, 0.0);

    assert_eq!(
        selected_ids(&editor),
        vec![b_id, c_id],
        "绘制后选中不得漂移"
    );
    assert_eq!(selected_indices(&editor), vec![1, 2], "索引应右移 1");
}

#[test]
fn test_single_drag_preserves_selection() {
    let mut editor = Editor::default();
    let (a_id, _, _) = seed_three(&mut editor);
    editor.selection_insert(0); // 选中 A（拖动目标）

    let count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], count, 0, 60);
    drag.set_delta(2000, 0); // A 从 tick 0 → 2000

    assert!(editor.finalize_dragging(0, drag));
    // 有序恢复后 A 落到新索引 2（越过 B/C），选中必须仍指向 A（id 口径）
    assert_eq!(selected_ids(&editor), vec![a_id], "拖动后选中应仍指向 A");
    assert_eq!(selected_indices(&editor), vec![2], "A 应落到新位置索引 2");
    let a = editor
        .editor_state
        .data
        .track_notes(1)
        .iter()
        .find(|n| n.id == a_id)
        .expect("A 存在");
    assert_eq!(a.start_tick, 2000, "拖动 delta 应已应用");
}
