//! 延迟提交方案测试：pending_drag_state + 累积 delta
//!
//! 覆盖：
//! - `DraggingSelection` 松手后保存到 `pending_drag_state`（不立即 apply）
//! - 累积 delta：多次拖动同一选区，delta 叠加
//! - `commit_pending_drag`：apply 到 notes 并清空 pending
//! - `commit_current_edit` 在 pending 状态下提交
//! - `is_editing` / `has_pending_drag` 状态判定

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use lumino_core::DragState;

/// 模拟 pressed.rs 中批量拖动入口：push_history（首次）+ 进入 DraggingSelection
///
/// **累积模式**：如果 pending_drag_state 已存在，不重复 push_history
/// （一次逻辑操作只产生一条 history 记录）
fn start_dragging_selection(
    editor: &mut Editor,
    indices: impl IntoIterator<Item = usize>,
    delta_tick: i64,
    delta_key: i16,
) {
    let note_count = editor.editor_state.data.notes.len();
    // 累积模式：pending 存在时不重复 push_history
    if editor.pending_drag_state.is_none() {
        editor.push_history();
    }
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}

// ===== 初始状态判定 =====

#[test]
fn test_pending_drag_state_initial_is_none() {
    let editor = Editor::new();
    assert!(editor.pending_drag_state.is_none());
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_is_editing_returns_false_when_no_pending_and_idle() {
    let editor = Editor::new();
    assert!(!editor.is_editing());
}

// ===== commit_pending_drag 行为 =====

#[test]
fn test_commit_pending_drag_when_none_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    assert!(!editor.commit_pending_drag());
    assert!(editor.pending_drag_state.is_none());
}

#[test]
fn test_commit_pending_drag_with_zero_delta_returns_false_and_clears() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    // 直接设置一个 delta 为零的 pending
    let zero_drag = DragState::from_single(0, 1, 0, 60); // delta = (0, 0)
    editor.pending_drag_state = Some(zero_drag);

    assert!(!editor.commit_pending_drag(), "delta 零应返回 false");
    assert!(editor.pending_drag_state.is_none(), "pending 应被清空");
    // notes 未被修改
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 60);
}

#[test]
fn test_commit_pending_drag_applies_delta_to_notes() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(240.0, 62, 240.0));

    // 选中索引 0，delta=(200, 7)
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    editor.pending_drag_state = Some(drag);

    assert!(editor.commit_pending_drag());
    assert_eq!(editor.editor_state.data.notes[0].tick, 200.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 67);
    // note 1 未选中，不变
    assert_eq!(editor.editor_state.data.notes[1].tick, 240.0);
    assert_eq!(editor.editor_state.data.notes[1].key, 62);
    // pending 已清空
    assert!(editor.pending_drag_state.is_none());
}

#[test]
fn test_commit_pending_drag_clamps_negative_tick_to_zero() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(50.0, 60, 480.0));

    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(-200, 0); // 50 - 200 = -150，应 clamp 到 0
    editor.pending_drag_state = Some(drag);

    assert!(editor.commit_pending_drag());
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0, "应 clamp 到 0");
}

// ===== is_editing / has_pending_drag 在 pending 状态 =====

#[test]
fn test_is_editing_returns_true_when_pending_drag_exists() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
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
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    // 模拟一次移动并提交到 history
    {
        let note = editor
            .editor_state
            .data
            .notes
            .get_mut(0)
            .expect("note[0] 应存在");
        note.tick = 100.0;
    }
    editor.editor_state.data.sync_track_notes();

    // 现在 pending_drag_state 存在（模拟用户拖动后未点击空白处）
    editor.pending_drag_state = Some(DragState::from_single(0, 1, 0, 60));
    assert!(!editor.undo(), "pending 状态下 Undo 应被拦截");
}

// ===== 延迟提交流程：DraggingSelection 松手 → pending → commit =====

#[test]
fn test_dragging_selection_release_saves_to_pending_not_notes() {
    // 松手时不 apply 到 notes，只保存到 pending_drag_state
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    start_dragging_selection(&mut editor, [0], 100, 5);
    // 松手
    editor.handle_released();

    // notes 未被修改（延迟提交）
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 60);
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
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    start_dragging_selection(&mut editor, [0], 0, 0); // delta 零
    editor.handle_released();

    assert!(!editor.has_pending_drag(), "delta 零不应保存 pending");
}

// ===== 累积 delta：多次拖动叠加 =====

#[test]
fn test_accumulated_delta_two_drags_sum_up() {
    // 两次拖动同一选区：第一次 delta=(100,5)，第二次 delta=(50,3)
    // 累积后 pending delta 应为 (150,8)
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

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
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 60);

    // 提交累积 delta
    assert!(editor.commit_pending_drag());
    assert_eq!(editor.editor_state.data.notes[0].tick, 150.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 68); // 60 + 8
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_accumulated_delta_three_drags_with_negative() {
    // 三次拖动：(+100,+5) → (+50,-3) → (-200,0)
    // 累积 = (-50, 2)
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(100.0, 60, 480.0));

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
    assert!(editor.commit_pending_drag());
    assert_eq!(editor.editor_state.data.notes[0].tick, 50.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 62); // 60 + 2
}

#[test]
fn test_accumulated_delta_only_one_history_push() {
    // 累积模式下，多次拖动只 push 一次 history（一次逻辑操作一条记录）
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

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

// ===== commit_current_edit 提交 pending =====

#[test]
fn test_commit_current_edit_commits_pending_drag() {
    // pending 状态下调用 commit_current_edit 应提交 pending（Save/Play/Export 前的 fallback）
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    start_dragging_selection(&mut editor, [0], 100, 5);
    editor.handle_released();
    assert!(editor.has_pending_drag());

    // 模拟用户按 Save：commit_current_edit 应提交 pending
    assert!(editor.commit_current_edit());
    assert_eq!(editor.editor_state.data.notes[0].tick, 100.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 65);
    assert!(!editor.has_pending_drag(), "commit 后 pending 应清空");
    assert!(!editor.is_editing(), "commit 后应退出编辑状态");
}

#[test]
fn test_commit_current_edit_when_pending_only_returns_true() {
    // pending 状态下（edit_state=Idle）commit_current_edit 也应触发提交
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

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
    assert_eq!(editor.editor_state.data.notes[0].tick, 80.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 64);
    assert!(!editor.has_pending_drag());
}

// ===== flush_pending_drag（点击空白处提交）行为验证 =====

#[test]
fn test_idle_with_pending_then_commit_clears_pending() {
    // 模拟用户拖动后松手（pending 状态）→ 点击空白处提交
    // flush_pending_drag 是 private，通过 commit_pending_drag 验证
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(100.0, 62, 240.0));

    start_dragging_selection(&mut editor, [0, 1], 50, 2);
    editor.handle_released();

    // 模拟点击空白处：commit_pending_drag
    assert!(editor.commit_pending_drag());
    assert_eq!(editor.editor_state.data.notes[0].tick, 50.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 62); // 60 + 2
    assert_eq!(editor.editor_state.data.notes[1].tick, 150.0); // 100 + 50
    assert_eq!(editor.editor_state.data.notes[1].key, 64); // 62 + 2
    assert!(!editor.has_pending_drag());
}
