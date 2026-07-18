//! ghost 拖动测试：is_editing 判定 + commit_current_edit 行为
//!
//! 覆盖：
//! - `Editor::is_editing()` 对各种 `EditState` 的判定
//! - `Editor::commit_current_edit()` 在编辑/非编辑状态下的行为

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use lumino_core::DragState;

// ===== is_editing 判定测试 =====

#[test]
fn test_is_editing_idle_returns_false() {
    let editor = Editor::new();
    assert!(!editor.is_editing());
}

#[test]
fn test_is_editing_selecting_returns_false() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick: 0.0,
        start_key: 60,
        current_tick: 100.0,
        current_key: 70,
    };
    assert!(!editor.is_editing(), "Selecting 不属于数据编辑状态");
}

#[test]
fn test_is_editing_scrubbing_returns_false() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::Scrubbing;
    assert!(!editor.is_editing());
}

#[test]
fn test_is_editing_pending_drag_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::PendingDrag {
        note_index: 0,
        start_pos: (10.0, 20.0),
        original_tick: 0.0,
        original_key: 60,
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_drawing_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::Drawing {
        start_tick: 0.0,
        key: 60,
        current_tick: 480.0,
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_dragging_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: DragState::from_single(0, 1, 0, 60),
        last_played_key: 60,
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_dragging_selection_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection {
        drag_state: DragState::from_single(0, 1, 0, 60),
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_resizing_start_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::ResizingStart {
        note_index: 0,
        original_tick: 0.0,
        original_length: 480.0,
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_resizing_end_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state = EditState::ResizingEnd {
        note_index: 0,
        original_length: 480.0,
    };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_resizing_selection_start_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state =
        EditState::ResizingSelectionStart { last_tick: 100.0 };
    assert!(editor.is_editing());
}

#[test]
fn test_is_editing_resizing_selection_end_returns_true() {
    let mut editor = Editor::new();
    editor.editor_state.interaction.edit_state =
        EditState::ResizingSelectionEnd { last_tick: 100.0 };
    assert!(editor.is_editing());
}

// ===== commit_current_edit 测试 =====

#[test]
fn test_commit_current_edit_when_idle_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    assert!(!editor.commit_current_edit());
    assert_eq!(editor.editor_state.data.notes.len(), 1);
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));
}

#[test]
fn test_commit_current_edit_when_selecting_does_not_commit() {
    // Selecting 不是数据编辑状态，commit 应返回 false（不触发 handle_released 提交数据）
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick: 0.0,
        start_key: 60,
        current_tick: 100.0,
        current_key: 70,
    };
    assert!(!editor.commit_current_edit());
}

#[test]
fn test_commit_current_edit_when_dragging_commits_and_returns_true() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    let mut drag = DragState::from_single(0, 1, 0, 60);
    drag.set_delta(100, 5);
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: drag,
        last_played_key: 60,
    };

    assert!(editor.commit_current_edit());
    // delta 应已被应用到 notes
    assert_eq!(editor.editor_state.data.notes[0].tick, 100.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 65);
    // edit_state 应被重置为 Idle
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));
}

#[test]
fn test_commit_current_edit_when_dragging_selection_commits_all_selected() {
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
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 240.0));

    // 选中索引 0 和 2
    let mut drag = DragState::from_indices([0, 2], 3, 0, 60);
    drag.set_delta(200, 7);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };

    assert!(editor.commit_current_edit());
    let notes = &editor.editor_state.data.notes;
    // note 0: 0,60 -> 200,67
    assert_eq!(notes[0].tick, 200.0);
    assert_eq!(notes[0].key, 67);
    // note 1: 未选中，不变
    assert_eq!(notes[1].tick, 240.0);
    assert_eq!(notes[1].key, 62);
    // note 2: 480,64 -> 680,71
    assert_eq!(notes[2].tick, 680.0);
    assert_eq!(notes[2].key, 71);
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));
}

#[test]
fn test_commit_current_edit_with_zero_delta_returns_true_but_no_change() {
    // 即使 delta 为零，commit_current_edit 仍然返回 true（因为 is_editing() 为 true）
    // 但 notes 不会被修改
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    let drag = DragState::from_single(0, 1, 0, 60); // delta = (0, 0)
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: drag,
        last_played_key: 60,
    };

    assert!(editor.commit_current_edit(), "is_editing=true 应返回 true");
    // notes 不变
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
    assert_eq!(editor.editor_state.data.notes[0].key, 60);
    // edit_state 已被重置为 Idle（handle_released 已执行）
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Idle
    ));
}
