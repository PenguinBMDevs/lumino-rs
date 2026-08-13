//! 交互相关测试（选中、删除、音轨切换、工具切换）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_message::Tool;

/// 测试音符选中功能
#[test]
fn test_note_selection() {
    let mut editor = Editor::new();

    // 添加一些音符
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(480.0, 64, 480.0)],
    );

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
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(480.0, 64, 480.0)],
    );

    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);

    // 删除第一个音符
    editor.delete_note_by_index(0);

    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
    assert_eq!(editor.editor_state.data.get_note_view(0).expect("第 1 个音符视图应存在").key, 64); // 第二个音符变成第一个
}

/// 测试音轨切换
#[test]
fn test_track_switching() {
    let mut editor = Editor::new();

    // 在当前音轨（0）添加音符，document 含 2 轨（供切换）
    test_helpers::seed_notes(&mut editor, 2, 0, &[Note::new(0.0, 60, 480.0)]);

    // 切换到音轨 1
    editor.switch_to_track(1);

    assert_eq!(editor.current_track(), 1);
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0); // 新音轨应该为空

    // 在音轨 1 添加音符
    editor.editor_state.data.insert_note(
        editor.editor_state.data.current_track,
        Note::new(0.0, 64, 480.0),
    );

    // 切换回音轨 0
    editor.switch_to_track(0);

    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
    assert_eq!(editor.editor_state.data.get_note_view(0).expect("第 1 个音符视图应存在").key, 60); // 应该恢复原来的音符
}

/// 测试音轨切换不会误设 notes_changed
///
/// 切轨只是切换当前显示的音轨（current_track_notes 换轨），并非用户编辑，不能设置 notes_changed，
/// 否则 Host::handle_action 会误判为脏音轨并触发高精度洋葱皮覆盖层/重生。
#[test]
fn test_track_switching_does_not_set_notes_changed() {
    let mut editor = Editor::new();

    test_helpers::seed_notes(&mut editor, 2, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.mark_notes_changed();
    assert!(editor.notes_changed());
    editor.clear_notes_changed();
    assert!(!editor.notes_changed());

    // 切换到另一音轨后，notes_changed 必须保持 false
    editor.switch_to_track(1);
    assert_eq!(editor.current_track(), 1);
    assert!(!editor.notes_changed(), "切轨不应设置 notes_changed 标志");

    // 但空间索引必须标记为脏，以便后续命中测试重建
    assert!(
        editor.spatial.note_index_dirty.get(),
        "切轨必须标记空间索引为脏"
    );
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

/// 测试全选（小数据量，fallback 路径）
#[test]
fn test_select_all_notes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(480.0, 64, 480.0)],
    );

    editor.select_all_notes();

    assert_eq!(editor.selected_notes_count(), 2);
    assert!(editor.is_note_selected(0));
    assert!(editor.is_note_selected(1));
}

/// 测试全选（大数据量路径）
#[test]
fn test_select_all_notes_with_notestore() {
    let mut editor = Editor::new();
    // 2026-08 单一权威源：NoteStore 已删除（is_note_store_enabled 恒 false），
    // 大量音符经 document 构造，select_all_notes 走全量选择路径。
    // 添加超过旧 NOTE_STORE_THRESHOLD (10_000) 的音符，验证大数据量全选
    let count = 10_050usize;
    let notes: Vec<Note> = (0..count)
        .map(|i| Note::new(i as f32, 60 + (i % 12) as u16, 1.0))
        .collect();
    test_helpers::seed_notes(&mut editor, 1, 0, &notes);
    assert!(!editor.editor_state.data.is_note_store_enabled());

    editor.select_all_notes();

    assert_eq!(editor.selected_notes_count(), count);
    assert!(editor.is_note_selected(0));
    assert!(editor.is_note_selected(count - 1));
    assert!(editor.is_note_selected(count / 2));
}
