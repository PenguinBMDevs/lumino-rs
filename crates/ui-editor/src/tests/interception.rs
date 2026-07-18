//! 编辑拦截逻辑测试：Undo/Redo 在编辑状态下被拦截 + DragState 集成
//!
//! 覆盖：
//! - `Editor::undo()` / `redo()` 在编辑状态下被拦截（不修改 history / notes / edit_state）
//! - DragState 在 Editor 上下文中的 ghost 行为（clamp、批量选择）

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use lumino_core::DragState;

// ===== Undo / Redo 拦截测试 =====

#[test]
fn test_undo_when_idle_works() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 62, 240.0));

    assert!(editor.undo(), "非编辑状态 undo 应成功");
    assert_eq!(editor.editor_state.data.notes.len(), 1);
}

#[test]
fn test_undo_when_dragging_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 62, 240.0));

    // 进入 Dragging 状态
    let mut drag = DragState::from_single(0, 2, 0, 60);
    drag.set_delta(50, 3);
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: drag,
        last_played_key: 60,
    };

    assert!(!editor.undo(), "编辑状态 undo 应被拦截");
    // notes 不变（拦截后不应有任何修改）
    assert_eq!(editor.editor_state.data.notes.len(), 2);
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
    assert_eq!(editor.editor_state.data.notes[1].tick, 480.0);
    // edit_state 仍是 Dragging（未被 undo 触碰）
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Dragging { .. }
    ));
}

#[test]
fn test_undo_when_dragging_selection_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();

    editor.editor_state.interaction.edit_state = EditState::DraggingSelection {
        drag_state: DragState::from_single(0, 1, 0, 60),
    };

    assert!(!editor.undo(), "DraggingSelection 状态 undo 应被拦截");
    assert_eq!(editor.editor_state.data.notes.len(), 1);
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::DraggingSelection { .. }
    ));
}

#[test]
fn test_undo_when_drawing_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();

    editor.editor_state.interaction.edit_state = EditState::Drawing {
        start_tick: 0.0,
        key: 60,
        current_tick: 480.0,
    };

    assert!(!editor.undo(), "Drawing 状态 undo 应被拦截");
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Drawing { .. }
    ));
}

#[test]
fn test_undo_when_resizing_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();

    editor.editor_state.interaction.edit_state = EditState::ResizingEnd {
        note_index: 0,
        original_length: 480.0,
    };

    assert!(!editor.undo(), "Resizing 状态 undo 应被拦截");
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::ResizingEnd { .. }
    ));
}

#[test]
fn test_redo_when_idle_works() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 62, 240.0));
    assert!(editor.undo());
    assert!(editor.redo(), "非编辑状态 redo 应成功");
    assert_eq!(editor.editor_state.data.notes.len(), 2);
}

#[test]
fn test_redo_when_dragging_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 62, 240.0));
    let _ = editor.undo();

    // 进入 Dragging 状态
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: DragState::from_single(0, 1, 0, 60),
        last_played_key: 60,
    };

    assert!(!editor.redo(), "编辑状态 redo 应被拦截");
    // notes 不变
    assert_eq!(editor.editor_state.data.notes.len(), 1);
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Dragging { .. }
    ));
}

#[test]
fn test_redo_when_drawing_returns_false() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor.push_history();
    let _ = editor.undo();

    editor.editor_state.interaction.edit_state = EditState::Drawing {
        start_tick: 0.0,
        key: 60,
        current_tick: 480.0,
    };

    assert!(!editor.redo(), "Drawing 状态 redo 应被拦截");
    assert!(matches!(
        editor.editor_state.interaction.edit_state,
        EditState::Drawing { .. }
    ));
}

// ===== DragState 集成测试（在 Editor 上下文中验证 ghost 行为）=====

#[test]
fn test_ghost_position_clamp_in_editor_default_view() {
    // Editor 默认 visible_key_count=128, 所以 max_key=127
    let mut drag = DragState::default();
    drag.set_delta(0, 200); // 远超 127
    let (_, ghost_key) = drag.ghost_position(0.0, 60, 127);
    assert_eq!(ghost_key, 127, "ghost_key 应被 clamp 到 max_key=127");
}

#[test]
fn test_dragging_commit_clamps_key_to_visible_range() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 100, 480.0));

    // visible_key_count 默认 128, max_key = 127
    let mut drag = DragState::from_single(0, 1, 0, 100);
    drag.set_delta(0, 100); // 100 + 100 = 200, 应 clamp 到 127
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: drag,
        last_played_key: 100,
    };

    assert!(editor.commit_current_edit());
    assert_eq!(editor.editor_state.data.notes[0].key, 127);
}

#[test]
fn test_dragging_commit_clamps_negative_tick_to_zero() {
    let mut editor = Editor::new();
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(50.0, 60, 480.0));

    let mut drag = DragState::from_single(0, 1, 50, 60);
    drag.set_delta(-1000, 0); // 50 + (-1000) = -950, 应 clamp 到 0
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index: 0,
        drag_state: drag,
        last_played_key: 60,
    };

    assert!(editor.commit_current_edit());
    assert_eq!(editor.editor_state.data.notes[0].tick, 0.0);
}

#[test]
fn test_dragging_selection_commit_only_modifies_selected() {
    let mut editor = Editor::new();
    for i in 0..5u16 {
        editor
            .editor_state
            .data
            .notes
            .push_back(Note::new(i as f32 * 100.0, 60 + i, 80.0));
    }

    // 选中偶数索引：0, 2, 4
    let mut drag = DragState::from_indices([0, 2, 4], 5, 0, 60);
    drag.set_delta(50, 1);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };

    assert!(editor.commit_current_edit());
    let notes = &editor.editor_state.data.notes;
    // 选中：0,2,4 -> +50, +1
    assert_eq!(notes[0].tick, 50.0);
    assert_eq!(notes[0].key, 61);
    assert_eq!(notes[2].tick, 250.0);
    assert_eq!(notes[2].key, 63);
    assert_eq!(notes[4].tick, 450.0);
    assert_eq!(notes[4].key, 65);
    // 未选中：1,3 不变
    assert_eq!(notes[1].tick, 100.0);
    assert_eq!(notes[1].key, 61);
    assert_eq!(notes[3].tick, 300.0);
    assert_eq!(notes[3].key, 63);
}
