//! 交互相关测试（选中、删除、音轨切换、工具切换）

use crate::Editor;
use crate::note::Note;
use lumino_message::Tool;

/// 测试音符选中功能
#[test]
fn test_note_selection() {
    let mut editor = Editor::new();

    // 添加一些音符
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 480.0));

    // 选中第一个音符（通过 editor_state）
    editor.editor_state.interaction.selected_notes.insert(0);

    assert!(editor.is_note_selected(0));
    assert!(!editor.is_note_selected(1));
    assert_eq!(editor.selected_notes_count(), 1);

    // 清除选中
    editor.clear_selection();
    assert_eq!(editor.selected_notes_count(), 0);
}

/// 测试音符删除
#[test]
fn test_note_deletion() {
    let mut editor = Editor::new();

    // 添加音符
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 480.0));

    assert_eq!(editor.editor_state.data.notes.len(), 2);

    // 删除第一个音符
    editor.delete_note_by_index(0);

    assert_eq!(editor.editor_state.data.notes.len(), 1);
    assert_eq!(editor.editor_state.data.notes[0].key, 64); // 第二个音符变成第一个
}

/// 测试音轨切换
#[test]
fn test_track_switching() {
    let mut editor = Editor::new();

    // 在当前音轨添加音符
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    // 切换到音轨 1
    editor.switch_to_track(1);

    assert_eq!(editor.current_track(), 1);
    assert!(editor.editor_state.data.notes.is_empty()); // 新音轨应该为空

    // 在音轨 1 添加音符
    editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 64, 480.0));

    // 切换回音轨 0
    editor.switch_to_track(0);

    assert_eq!(editor.editor_state.data.notes.len(), 1);
    assert_eq!(editor.editor_state.data.notes[0].key, 60); // 应该恢复原来的音符
}

/// 测试工具设置
#[test]
fn test_tool_setting() {
    let mut editor = Editor::new();

    // 默认应该是指针工具
    assert_eq!(editor.current_tool(), Tool::Pointer);

    // 设置为铅笔工具
    editor.set_tool(Tool::Pencil);
    assert_eq!(editor.current_tool(), Tool::Pencil);

    // 添加选中状态
    editor.editor_state.interaction.selected_notes.insert(0);
    assert_eq!(editor.selected_notes_count(), 1);

    // 切换到非指针工具应该清除选中
    editor.set_tool(Tool::Eraser);
    assert_eq!(editor.selected_notes_count(), 0);
}

/// 测试全选
#[test]
fn test_select_all_notes() {
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
        .push_back(Note::new(480.0, 64, 480.0));

    editor.select_all_notes();

    assert_eq!(editor.selected_notes_count(), 2);
    assert!(editor.is_note_selected(0));
    assert!(editor.is_note_selected(1));
}
