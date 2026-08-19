use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== 复制后只保留最新件（副本）框选（原件不再框选，用户要求） =====

#[test]
fn test_selection_box_rects_only_copy_box() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：delta=600 → 副本位于 tick [600, 1080]，与原件 [0, 480] 完全分离
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(600, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 单框 = 副本框 [600, 1080]（最新件框选；原件不再框选）
    let rects = editor.get_selection_box_rects();
    assert_eq!(rects.len(), 1, "复制模式应只返回一个副本框（最新件框选）");
    let v = &editor.editor_state.view;
    let copy_x = v.tick_to_x(600.0);
    let copy_end_x = v.tick_to_x(1080.0);
    let (c_x1, c_x2, _, _) = rects[0];
    assert!(
        (c_x1 - copy_x).abs() < 0.001,
        "副本框左边界 = 副本起点（实际 {}, 期望 {}）",
        c_x1,
        copy_x
    );
    assert!(
        (c_x2 - copy_end_x).abs() < 0.001,
        "副本框右边界 = 副本终点（实际 {}, 期望 {}）",
        c_x2,
        copy_end_x
    );
    // 兼容入口返回并集（此时 = 副本框本身）
    let (u_x1, u_x2, _, _) = editor.get_selection_box_bounds().expect("应有并集框");
    assert!(
        (u_x1 - copy_x).abs() < 0.001 && (u_x2 - copy_end_x).abs() < 0.001,
        "兼容入口 bounds 应为副本框 [{}, {}]，实际 [{}, {}]",
        copy_x,
        copy_end_x,
        u_x1,
        u_x2
    );
}

#[test]
fn test_selection_box_hit_test_copy_box_only() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：delta=600 → 副本位于 tick [600, 1080]，与原件 [0, 480] 完全分离
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(600, 0);
    editor.pending_copy_drag_state = Some(drag);

    let v = &editor.editor_state.view;
    let copy_center_y = v.key_to_y(60) + v.zoom_y / 2.0;
    // 副本中心（tick 840）→ Inside（副本框命中 = 最新件框选）
    let copy_center_x = v.tick_to_x(840.0);
    assert_eq!(
        editor.hit_test_selection_box(iced_core::Point::new(copy_center_x, copy_center_y)),
        Some(crate::SelectionHitType::Inside),
        "副本位置应命中副本框内部"
    );
    // 原件中心（tick 240）→ 不命中任何框（原件不再框选，用户要求）
    let origin_center_x = v.tick_to_x(240.0);
    assert_eq!(
        editor.hit_test_selection_box(iced_core::Point::new(origin_center_x, copy_center_y)),
        None,
        "原件位置不应命中任何选择框（原件不保留框选）"
    );

    // 副本框内 Ctrl+拖动 → 复制下一份（DraggingSelectionCopy）
    editor.set_ctrl_pressed(true);
    editor.handle_tool_pressed(
        iced_core::Point::new(copy_center_x, copy_center_y),
        false,
        840.0,
        60,
    );
    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelectionCopy { .. } => {}
        other => panic!(
            "Ctrl+拖动副本框应进入 DraggingSelectionCopy（复制下一份），实际 {:?}",
            other
        ),
    }
    // 内存未写入：pending 保留、document 未变（先 UI 后内存）
    assert!(
        editor.pending_copy_drag_state.is_some(),
        "再次 Ctrl 拖动不应提交 pending 复制"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "document 不应被写入"
    );

    // 原件位置（无框）Ctrl+按下 → priority 2 单音符命中 → 单音符编辑（移动原件）
    editor.handle_tool_pressed(
        iced_core::Point::new(origin_center_x, copy_center_y),
        false,
        240.0,
        60,
    );
    match editor.editor_state.interaction.edit_state {
        EditState::Dragging { .. } | EditState::PendingDrag { .. } => {}
        other => panic!("原件位置应进入单音符编辑（移动原件），实际 {:?}", other),
    }
}
