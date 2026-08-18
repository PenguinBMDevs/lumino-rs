use super::release_copy_drag;
use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== 连续复制（松手即提交：每次复制独立真实化，从副本继续复制） =====

#[test]
fn test_released_copy_drag_accumulates_delta() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 第一次复制：副本偏移 100 → 立即提交
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert!(editor.pending_copy_drag_state.is_none());

    // 第二次复制（连续复制）：副本已真实化并选中（selected = 副本索引），
    // 从副本位置（tick 100）继续拖动 50 → 新副本 = 副本 + 50 = 原件 + 150
    // 直接以「选中索引」构造 drag（模拟 pressed 时 get_selected_indices() = 副本）
    let copy_idx = editor
        .get_selected_indices()
        .first()
        .copied()
        .expect("复制提交后副本应选中");
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(copy_idx)
            .expect("note should exist")
            .tick,
        100.0,
        "提交后选中应指向副本（tick 100）"
    );
    let mut d2 = DragState::from_indices([copy_idx], 2, 100, 60);
    d2.set_delta(50, 0);
    release_copy_drag(&mut editor, d2);

    // 松手即提交：内存 = 原件 + 副本1(100) + 副本2(150)
    assert_eq!(editor.editor_state.data.current_track_note_count(), 3);
    let ticks: Vec<u32> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert_eq!(ticks, vec![0, 100, 150], "连续复制：100 + 50 = 150");
}

#[test]
fn test_released_copy_drag_accumulates_delta_across_axes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 第一次复制：tick+100, key+5 → 立即提交
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 5);
    release_copy_drag(&mut editor, d1);
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);

    // 第二次复制：从副本位置（tick 100, key 65）再拖 tick-30, key+2
    // → 新副本 = 副本 + (-30, +2) = 原件 + (70, 7)
    let copy_idx = editor
        .get_selected_indices()
        .first()
        .copied()
        .expect("复制提交后副本应选中");
    let mut d2 = DragState::from_indices([copy_idx], 2, 100, 65);
    d2.set_delta(-30, 2);
    release_copy_drag(&mut editor, d2);

    assert_eq!(editor.editor_state.data.current_track_note_count(), 3);
    let copy2 = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .find(|n| n.start_tick == 70)
        .expect("副本2（tick 70）应存在");
    assert_eq!(copy2.key, 67, "累积 key: 5 + 2 = 7");
}

// ===== BUG 复现：复制提交后，无 Ctrl 在副本框内拖动（移动模式） =====
// 期望：副本（最新件）跟随鼠标移动，原件不受影响（原件不再框选）

#[test]
fn test_move_drag_from_original_with_pending_copy_moves_both() {
    let mut editor = Editor::new();
    // 设置视口尺寸（测试环境无真实窗口，默认 size_x=0 导致可见 tick 范围为 0）
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    // 先 Ctrl+复制：delta=(100, 0)，副本立即真实化并选中（tick 100）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);

    // 无 Ctrl，在副本框内（tick 340 中心）按下 → DraggingSelection（移动副本）
    let copy_x = editor.editor_state.view.tick_to_x(340.0);
    let copy_y = editor.editor_state.view.key_to_y(60) + editor.editor_state.view.zoom_y / 2.0;
    let copy_center = iced_core::Point::new(copy_x, copy_y);
    editor.set_ctrl_pressed(false);
    editor.handle_tool_pressed(copy_center, false, 340.0, 60);

    match &editor.editor_state.interaction.edit_state {
        EditState::DraggingSelection { drag_state } => {
            assert_eq!(drag_state.delta_tick, 0, "按下时 delta 应为 0");
            assert!(
                !drag_state.selected.is_empty() && drag_state.selected[1],
                "副本索引（1）应被选中参与拖动"
            );
        }
        other => panic!("应进入 DraggingSelection（移动），实际 {:?}", other),
    }

    // 拖动 +50 tick：340 → 390
    let moved_x = editor.editor_state.view.tick_to_x(390.0);
    editor.handle_moved(iced_core::Point::new(moved_x, copy_y));

    if let EditState::DraggingSelection { drag_state } = &editor.editor_state.interaction.edit_state
    {
        assert_eq!(drag_state.delta_tick, 50, "拖动后 drag_state.delta 应为 50");
    }

    // 渲染层验证：原件(0) 不动，副本 ghost 在 tick 150
    let mut visible: Vec<(f32, u16, f32)> = Vec::new();
    editor.collect_visible_note_data(&mut visible, None, 0.0);
    let mut ticks: Vec<f32> = visible.iter().map(|(t, _, _)| *t).collect();
    ticks.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(
        ticks,
        vec![0.0, 150.0],
        "原件(0)保持、副本(150)跟随拖动，实际 {:?}",
        ticks
    );
}
