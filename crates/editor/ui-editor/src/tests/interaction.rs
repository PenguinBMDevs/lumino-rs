//! 交互相关测试（选中、删除、音轨切换、工具切换）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。

use std::collections::HashSet;

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use iced_core::Point;
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
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        64
    ); // 第二个音符变成第一个
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
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    ); // 应该恢复原来的音符
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

/// BUG 复现：批量框选后按 Delete 键应删除所有选中音符
///
/// 模拟完整用户操作流：指针工具 → 空白处按下（开始框选）→ 拖动（选中覆盖音符）
/// → 松手 → 按 Delete。验证框选后 Delete 能删除全部选中音符。
#[test]
fn test_delete_after_box_select() {
    let mut editor = Editor::new();
    use iced_core::Point;
    use lumino_ui_core::message::EditorAction;

    // 3 个音符：tick 0/480/960，key 60/64/67
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(480.0, 64, 480.0),
            Note::new(960.0, 67, 480.0),
        ],
    );

    let view = editor.editor_state.view.clone();
    // 空白处按下开始框选（key 70 区域无音符）
    let start_x = view.tick_to_x(100.0);
    let start_y = view.key_to_y(70) + view.zoom_y / 2.0;
    let tick = editor.x_to_tick(start_x);
    let snapped_tick = editor.snap_tick(tick);
    editor.handle_pointer_pressed(Point::new(start_x, start_y), None, snapped_tick);
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            crate::EditState::Selecting { .. }
        ),
        "按下空白处应进入框选状态"
    );

    // 拖动覆盖全部 3 个音符（tick 0~1440，key 50~70）
    let end_x = view.tick_to_x(1440.0);
    let end_y = view.key_to_y(50) + view.zoom_y / 2.0;
    editor.handle_moved(Point::new(end_x, end_y));

    // 松手结束框选
    editor.handle_released();
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            crate::EditState::Idle
        ),
        "框选松手后应回到 Idle"
    );
    assert_eq!(editor.selected_notes_count(), 3, "框选应选中全部 3 个音符");

    // 按 Delete 键 → 删除选中音符
    editor.handle_action(EditorAction::DeletePressed);

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "框选后按 Delete 应删除全部选中音符，实际剩余 {}",
        editor.editor_state.data.current_track_note_count()
    );
}

/// BUG 复现（精确场景）：框选后鼠标悬停在选中音符上，按 Delete 应删除全部选中音符
///
/// 用户真实操作：框选完成后鼠标通常停在框选终点（很可能落在选中音符上）。
/// 此时 hover_state 有值，`handle_delete_pressed` 的 hover 优先分支只删除
/// 悬停的那 1 个音符，批量选中的其余音符保留——Delete 键"不能删除(批量)音符"。
#[test]
fn test_delete_after_box_select_with_hover_on_selected_note() {
    let mut editor = Editor::new();
    use iced_core::Point;
    use lumino_ui_core::message::EditorAction;

    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(480.0, 64, 480.0),
            Note::new(960.0, 67, 480.0),
        ],
    );

    let view = editor.editor_state.view.clone();
    // 空白处按下开始框选
    let start_x = view.tick_to_x(100.0);
    let start_y = view.key_to_y(70) + view.zoom_y / 2.0;
    let tick = editor.x_to_tick(start_x);
    let snapped_tick = editor.snap_tick(tick);
    editor.handle_pointer_pressed(Point::new(start_x, start_y), None, snapped_tick);
    // 拖动覆盖全部 3 个音符
    let end_x = view.tick_to_x(1440.0);
    let end_y = view.key_to_y(50) + view.zoom_y / 2.0;
    editor.handle_moved(Point::new(end_x, end_y));
    // 松手结束框选
    editor.handle_released();
    assert_eq!(editor.selected_notes_count(), 3, "框选应选中全部 3 个音符");

    // 框选后鼠标悬停到第一个选中音符上（模拟用户操作后鼠标停在选区内）
    let hover_x = view.tick_to_x(240.0);
    let hover_y = view.key_to_y(60) + view.zoom_y / 2.0;
    editor.handle_moved(Point::new(hover_x, hover_y));
    assert!(
        editor.editor_state.interaction.hover_state.is_some(),
        "鼠标应悬停在选中音符上"
    );

    // 按 Delete → 应删除全部选中音符
    editor.handle_action(EditorAction::DeletePressed);

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "存在选中集合时按 Delete 应删除全部选中音符，实际剩余 {}",
        editor.editor_state.data.current_track_note_count()
    );
}

/// 工具设置
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

/// BUG 复现：批量框选右边缘拉伸后，主音轨应记录增量事件以供 GPU 刷新。
#[test]
fn test_batch_selection_resize_end_records_delta_events() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(480.0, 64, 480.0)],
    );

    // 选中全部音符并进入选择框右边缘拉伸状态
    editor.editor_state.interaction.selected_notes = HashSet::from([0, 1]);
    editor.editor_state.interaction.edit_state = crate::EditState::ResizingSelectionEnd {
        origin_tick: 0.0,
        last_tick: 0.0,
    };

    // 拖动到 tick 1920：两个音符长度都增加
    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(1920.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    editor.handle_moved(Point::new(x, y));

    // 拖动期间走 ghost 路径，不记录主音轨增量事件
    assert!(
        editor.editor_state.data.note_delta_events.is_empty(),
        "拉伸拖动期间不应记录主音轨事件"
    );

    // 松手后必须记录增量事件，否则 GPU 不会刷新
    editor.handle_released();
    assert!(
        !editor.editor_state.data.note_delta_events.is_empty(),
        "批量拉伸释放后应记录增量事件"
    );
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

    editor.select_all_notes();

    assert_eq!(editor.selected_notes_count(), count);
    assert!(editor.is_note_selected(0));
    assert!(editor.is_note_selected(count - 1));
    assert!(editor.is_note_selected(count / 2));
}
