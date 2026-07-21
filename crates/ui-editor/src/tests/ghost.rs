//! ghost 拖动测试：is_editing 判定 + commit_current_edit 行为
//!
//! 覆盖：
//! - `Editor::is_editing()` 对各种 `EditState` 的判定
//! - `Editor::commit_current_edit()` 在编辑/非编辑状态下的行为

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::rendering::ghost_delta_for_index;
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
        start_y: 0.0,
        current_y: 0.0,
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
        start_y: 0.0,
        current_y: 0.0,
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

// ===== ghost_delta_for_index 跨状态测试 =====

#[test]
fn test_ghost_delta_applies_pending_in_selecting_state() {
    // 复现：批量拖动松手后进入 pending 状态，用户点击空白处开始新框选（Selecting），
    // 此时 pending 异步提交尚未完成，音符不应回撤。
    let mut drag = DragState::from_indices([0, 2], 3, 0, 60);
    drag.set_delta(100, -5);

    let selecting_state = EditState::Selecting {
        start_tick: 0.0,
        start_key: 60,
        current_tick: 50.0,
        current_key: 70,
        start_y: 0.0,
        current_y: 0.0,
    };

    let delta = ghost_delta_for_index(0, &Some(drag.clone()), &selecting_state);
    assert_eq!(
        delta,
        Some((100, -5)),
        "Selecting 状态下 pending delta 仍应生效"
    );

    let delta = ghost_delta_for_index(1, &Some(drag), &selecting_state);
    assert_eq!(delta, None, "未选中的音符不应有 pending delta");
}

#[test]
fn test_ghost_delta_applies_pending_across_all_states() {
    // pending 代表已提交到后台但尚未完成的数据更新，在异步完成前应始终可见。
    let mut drag = DragState::from_single(0, 1, 0, 60);
    drag.set_delta(50, 3);

    let states = vec![
        EditState::Idle,
        EditState::Selecting {
            start_tick: 0.0,
            start_key: 60,
            current_tick: 10.0,
            current_key: 70,
            start_y: 0.0,
            current_y: 0.0,
        },
        EditState::Drawing {
            start_tick: 0.0,
            key: 60,
            current_tick: 10.0,
        },
        EditState::PendingDrag {
            note_index: 1,
            start_pos: (0.0, 0.0),
            original_tick: 10.0,
            original_key: 62,
        },
        EditState::ResizingStart {
            note_index: 1,
            original_tick: 10.0,
            original_length: 100.0,
        },
        EditState::ResizingEnd {
            note_index: 1,
            original_length: 100.0,
        },
        EditState::ResizingSelectionStart { last_tick: 10.0 },
        EditState::ResizingSelectionEnd { last_tick: 10.0 },
        EditState::Scrubbing,
    ];

    for state in states {
        let delta = ghost_delta_for_index(0, &Some(drag.clone()), &state);
        assert_eq!(
            delta,
            Some((50, 3)),
            "状态 {:?} 下 pending delta 仍应生效",
            state
        );
    }
}

#[test]
fn test_ghost_delta_accumulates_pending_and_current_drag() {
    // DraggingSelection 累积模式：pending delta + 当前 drag delta 同时生效
    let mut pending = DragState::from_indices([0], 1, 0, 60);
    pending.set_delta(50, 2);

    let mut current = DragState::from_indices([0], 1, 0, 60);
    current.set_delta(30, -1);

    let state = EditState::DraggingSelection {
        drag_state: current,
    };
    let delta = ghost_delta_for_index(0, &Some(pending), &state);
    assert_eq!(delta, Some((80, 1)), "pending 与当前 drag delta 应累加");
}
