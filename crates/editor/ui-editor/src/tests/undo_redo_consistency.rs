//! 撤销/重做一致性回归：还原位置精确 + 批量场景不失败
//!
//! 背景 BUG：撤销重做还原位置错误；批量操作下撤消重做失败。
//! 根因：
//! 1. 批量拖动双压栈（拖动时推快照 + 提交时推 MoveOp），多步移动的 undo 链出现空操作步，
//!    第二次 undo 无变化（用户感知的“批量撤销失败”）。修复为 MoveOp-only（拖动不推，提交推一条）。
//! 2. 幽灵态（松手未取消框选）拦截 Undo/Redo 返回 false。修复为先提交幽灵再撤销/重做，
//!    一次按键即回退刚提交的移动，redo 可恢复。
//! 3. 同值多份在 undo/redo 的选中重建中碰撞到同一索引丢一份（份数语义破坏）。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;
use lumino_note_core::history::OpKind;

/// 生产链路批量提交（不推快照，仅提交推 MoveOp）：拖动→松手→提交→落盘→清空选区
fn batch_move_commit(editor: &mut Editor, indices: Vec<usize>, delta_tick: i64, delta_key: i16) {
    let note_count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
    editor.handle_released();
    assert!(editor.commit_pending_drag(), "批量提交应启动");
    editor.drain_async_commit();
    editor.selection_clear();
}

fn track_ticks(editor: &Editor) -> Vec<u32> {
    editor
        .editor_state
        .data
        .track_notes(0)
        .iter()
        .map(|n| n.start_tick)
        .collect()
}

fn track_keys(editor: &Editor) -> Vec<u8> {
    editor
        .editor_state
        .data
        .track_notes(0)
        .iter()
        .map(|n| n.key)
        .collect()
}

// ── 批量移动 undo/redo 位置精确 ────────────────────────────────

#[test]
fn test_undo_redo_single_note_drag_restores_positions() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    editor.selection_insert(0);

    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    assert!(editor.finalize_dragging(0, drag), "单音符拖动应提交");
    assert_eq!(track_ticks(&editor), vec![100]);

    assert!(editor.undo(), "单音符拖动 undo 应成功");
    assert_eq!(track_ticks(&editor), vec![0], "undo 应回原位置");

    assert!(editor.redo(), "单音符拖动 redo 应成功");
    assert_eq!(track_ticks(&editor), vec![100], "redo 应回到移动位置");
}

#[test]
fn test_history_jump_across_single_note_move() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    editor.selection_insert(0);

    editor
        .editor_state
        .data
        .push_history_with_op_kind(OpKind::NoteCreate);
    editor.editor_state.data.insert_note(
        editor.editor_state.data.current_track,
        Note::new(500.0, 62, 100.0),
    );
    // 上面 push 的是创建前的快照；当前有 2 个音符，创建第 2 个音符为一步。
    // 再移动第 1 个音符，形成第二步。
    editor.selection_clear();
    editor.selection_insert(0);
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(100, 0);
    assert!(editor.finalize_dragging(0, drag));
    assert_eq!(track_ticks(&editor), vec![100, 500]);

    assert_eq!(editor.history_view().current, 2);
    assert!(editor.history_jump_to(1), "跳到移动前的步骤");
    assert_eq!(track_ticks(&editor), vec![0, 500], "移动应被撤销");

    assert!(editor.history_jump_to(2), "重做移动");
    assert_eq!(track_ticks(&editor), vec![100, 500], "移动应被重做");
}

#[test]
fn test_history_jump_across_batch_move() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    editor.selection_insert(0);
    batch_move_commit(&mut editor, vec![0], 100, 0);
    assert_eq!(track_ticks(&editor), vec![100]);
    assert_eq!(editor.history_view().current, 1);

    // 先清掉选区外点击产生的 pending 状态；这正是 history_jump_to 的守卫场景。
    editor.selection_clear();
    assert!(editor.history_jump_to(0), "跳到初始状态");
    assert_eq!(track_ticks(&editor), vec![0]);

    assert!(editor.history_jump_to(1), "重做批量移动");
    assert_eq!(track_ticks(&editor), vec![100]);
}

#[test]
fn test_undo_redo_batch_move_restores_positions() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 100.0),
            Note::new(500.0, 61, 100.0),
            Note::new(1800.0, 62, 100.0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);
    batch_move_commit(&mut editor, vec![0, 1], 1000, 0);
    assert_eq!(track_ticks(&editor), vec![1000, 1500, 1800]);

    assert!(editor.undo(), "undo 应成功");
    assert_eq!(track_ticks(&editor), vec![0, 500, 1800], "undo 应精确还原");

    assert!(editor.redo(), "redo 应成功");
    assert_eq!(
        track_ticks(&editor),
        vec![1000, 1500, 1800],
        "redo 应精确前进"
    );
}

#[test]
fn test_undo_redo_duplicate_move_keeps_copies() {
    // 同值双音符移动的 undo/redo 必须按份数处理两份
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::from_raw(0.0, 60, 100.0, 100, 0),
            Note::from_raw(0.0, 60, 100.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);
    batch_move_commit(&mut editor, vec![0, 1], 100, 0);
    assert_eq!(track_ticks(&editor), vec![100, 100]);

    assert!(editor.undo());
    assert_eq!(track_ticks(&editor), vec![0, 0], "undo 应还原两份");

    assert!(editor.redo());
    assert_eq!(track_ticks(&editor), vec![100, 100], "redo 应前进两份");
}

#[test]
fn test_undo_chain_multi_moves_each_step_reverts_one() {
    // 连续两次独立移动：两次 undo 各回退一步（双压栈时代第二次 undo 是空操作步，即报障点）
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(500.0, 61, 100.0)],
    );
    editor.selection_insert(0);
    batch_move_commit(&mut editor, vec![0], 100, 0);
    assert_eq!(track_ticks(&editor), vec![100, 500]);

    editor.selection_insert(1);
    batch_move_commit(&mut editor, vec![1], 200, 0);
    assert_eq!(track_ticks(&editor), vec![100, 700]);

    assert!(editor.undo());
    assert_eq!(
        track_ticks(&editor),
        vec![100, 500],
        "第一次 undo 回退第二次移动"
    );

    assert!(editor.undo());
    assert_eq!(
        track_ticks(&editor),
        vec![0, 500],
        "第二次 undo 回退第一次移动"
    );

    assert!(editor.redo());
    assert_eq!(track_ticks(&editor), vec![100, 500]);
    assert!(editor.redo());
    assert_eq!(track_ticks(&editor), vec![100, 700]);
}

#[test]
fn test_history_jump_across_create_entry_then_move() {
    use lumino_note_core::history::CreateOp;

    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.data.insert_note(
        editor.editor_state.data.current_track,
        Note::new(0.0, 60, 100.0),
    );
    let note = editor
        .editor_state
        .data
        .track_notes(editor.editor_state.data.current_track)
        .get(0)
        .copied()
        .expect("音符应存在");
    editor.editor_state.data.push_note_create(vec![CreateOp {
        track_id: editor.editor_state.data.current_track as u32,
        note,
    }]);
    editor.selection_insert(0);
    batch_move_commit(&mut editor, vec![0], 100, 0);
    assert_eq!(track_ticks(&editor), vec![100]);
    assert_eq!(editor.history_view().current, 2);

    assert!(editor.history_jump_to(1), "回到创建步");
    assert_eq!(track_ticks(&editor), vec![0]);
    assert!(editor.history_jump_to(2), "重做移动");
    assert_eq!(track_ticks(&editor), vec![100]);
    assert!(editor.history_jump_to(0), "回到初始状态");
    assert!(track_ticks(&editor).is_empty());
}

#[test]
fn test_history_jump_across_two_independent_moves() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(500.0, 61, 100.0)],
    );
    editor.selection_insert(0);
    batch_move_commit(&mut editor, vec![0], 100, 0);
    editor.selection_insert(1);
    batch_move_commit(&mut editor, vec![1], 200, 0);
    assert_eq!(track_ticks(&editor), vec![100, 700]);
    assert_eq!(editor.history_view().current, 2);

    assert!(editor.history_jump_to(0), "回到初始状态");
    assert_eq!(track_ticks(&editor), vec![0, 500]);

    assert!(editor.history_jump_to(1), "只重做第一次移动");
    assert_eq!(track_ticks(&editor), vec![100, 500]);

    assert!(editor.history_jump_to(2), "重做第二次移动");
    assert_eq!(track_ticks(&editor), vec![100, 700]);

    assert!(editor.history_jump_to(1), "从最新回退第二次移动");
    assert_eq!(track_ticks(&editor), vec![100, 500]);
    assert!(editor.history_jump_to(0), "继续回退第一次移动");
    assert_eq!(track_ticks(&editor), vec![0, 500]);
}

#[test]
fn test_history_jump_multi_note_pending_move_returns_to_creation() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor
        .editor_state
        .data
        .push_history_with_op_kind(OpKind::NoteCreate);
    editor.editor_state.data.insert_note(
        editor.editor_state.data.current_track,
        Note::new(0.0, 60, 100.0),
    );
    editor.editor_state.data.insert_note(
        editor.editor_state.data.current_track,
        Note::new(500.0, 62, 100.0),
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    let note_count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0, 1], note_count, 0, 60);
    drag.set_delta(100, 0);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
    editor.handle_released();
    assert!(editor.has_pending_drag());

    // 面板的创建步是 target 1；跳转前必须先 flush pending 的集体移动。
    assert!(editor.history_jump_to(1), "回到创建步");
    assert_eq!(track_ticks(&editor), vec![0, 500], "创建步应在移动前位置");
    assert_eq!(editor.history_view().current, 1);

    assert!(editor.history_jump_to(2), "前进到集体移动步");
    assert_eq!(track_ticks(&editor), vec![100, 600], "集体移动应重做");
    assert_eq!(editor.history_view().current, 2);
}

#[test]
fn test_history_jump_flushes_pending_batch_move() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    editor.selection_insert(0);

    let note_count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], note_count, 0, 60);
    drag.set_delta(100, 0);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
    editor.handle_released();
    assert!(editor.has_pending_drag(), "松手后应存在 pending 移动");
    assert_eq!(track_ticks(&editor), vec![0], "pending 尚未写入 document");

    assert!(
        editor.history_jump_to(1),
        "历史跳转必须先提交 pending，而不是直接拒绝"
    );
    assert!(!editor.has_pending_drag(), "跳转后 pending 应已 drain");
    assert_eq!(track_ticks(&editor), vec![100], "移动应已落盘");
    assert_eq!(editor.history_view().current, 1);

    assert!(editor.history_jump_to(0), "跳过 pending 后应能撤销移动");
    assert_eq!(track_ticks(&editor), vec![0], "移动应回原位置");
    assert!(editor.history_jump_to(1), "重做移动");
    assert_eq!(track_ticks(&editor), vec![100], "移动应重做");
}

// ── 幽灵态 undo/redo 不失败 ────────────────────────────────────

#[test]
fn test_undo_with_uncommitted_ghost_auto_commits_then_undoes() {
    // 拖动后未取消框选（幽灵态）直接 Ctrl+Z：旧实现拦截返回 false。
    // 新策略先提交幽灵再撤销，一次按键即回退本次移动，redo 可恢复。
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 100.0)]);
    editor.selection_insert(0);

    let note_count = editor.editor_state.data.current_track_note_count();
    let mut drag = DragState::from_indices([0], note_count, 0, 60);
    drag.set_delta(100, 0);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
    editor.handle_released();
    assert!(editor.has_pending_drag(), "松手后应有幽灵 pending");

    assert!(editor.undo(), "幽灵态 Undo 应先提交再撤销，不得失败");
    assert_eq!(track_ticks(&editor), vec![0], "撤销后应回到原位置");
    assert!(
        editor.pending_drag_state.is_none(),
        "撤销后幽灵应已清理（显示与内存一致）"
    );

    assert!(editor.redo(), "redo 应恢复刚提交的移动");
    assert_eq!(track_ticks(&editor), vec![100]);
}

// ── 快照类批量操作 undo/redo ───────────────────────────────────

#[test]
fn test_undo_redo_snapshot_batch_edit() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 100.0), Note::new(500.0, 61, 100.0)],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    assert_eq!(editor.apply_batch_edit("", "", "+12", "", 127), 2);
    assert_eq!(track_keys(&editor), vec![72, 73]);

    assert!(editor.undo(), "批量编辑 undo 应成功");
    assert_eq!(track_keys(&editor), vec![60, 61], "undo 应还原 key");

    assert!(editor.redo(), "批量编辑 redo 应成功");
    assert_eq!(track_keys(&editor), vec![72, 73], "redo 应前进 key");
}
