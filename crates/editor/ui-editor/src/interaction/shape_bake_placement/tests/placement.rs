//! DRAW-049 放置主流程：进入 / ✓ / × / 撤销 / 空白拖框与复位。

use super::{
    draw_rect, marquee_edit_state, pending_bounds, preview_note_snapshot, test_editor,
    track_note_count, undo_len,
};
use lumino_editor_state::PendingShapeRef;

#[test]
fn bake_enters_placement_without_writing_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = track_note_count(&editor, 1);

    assert!(
        editor.begin_shape_bake_placement(),
        "单形状 Bake 应进入放置确认态"
    );

    assert_eq!(track_note_count(&editor, 1), before, "✓ 前不得写文档");
    assert!(editor.bake_placement_active());
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(editor.is_shape_edit_armed(), "应自动进入 8 手柄编辑态");
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "待确认形状应转交给 pending composed"
    );
}

#[test]
fn placement_confirm_writes_notes_and_closes_session() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = track_note_count(&editor, 1);
    assert!(editor.begin_shape_bake_placement());

    assert!(editor.confirm_bake_placement());
    assert!(track_note_count(&editor, 1) > before, "✓ 应写入音符");
    assert!(!editor.bake_placement_active());
    assert!(editor.editor_state.pending_composed.is_empty());
}

#[test]
fn placement_cancel_discards_without_writing_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let before = track_note_count(&editor, 1);
    assert!(editor.begin_shape_bake_placement());

    editor.cancel_bake_placement();
    assert_eq!(track_note_count(&editor, 1), before, "× 不得写文档");
    assert!(!editor.bake_placement_active());
    assert!(editor.editor_state.pending_composed.is_empty());
}

#[test]
fn cancel_leaves_no_residue() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let notes_before = track_note_count(&editor, 1);
    let history_before = undo_len(&editor);
    assert!(editor.begin_shape_bake_placement());

    editor.handle_action(lumino_ui_core::message::EditorAction::ShapeToolCancel);

    assert!(!editor.bake_placement_active());
    assert!(editor.bake_placement.is_none());
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "pending 应清空"
    );
    assert!(
        editor.editor_state.shape_select.selection_len() == 0,
        "不得残留黄色选择框"
    );
    assert!(
        editor
            .editor_state
            .shape_select
            .pending_mirror(PendingShapeRef::Composed(0))
            .is_none(),
        "pending 镜像应一并清除"
    );
    assert_eq!(track_note_count(&editor, 1), notes_before, "× 不得写文档");
    assert_eq!(undo_len(&editor), history_before, "× 不得产生历史");
}

#[test]
fn confirm_writes_exactly_one_history_group_and_matches_preview() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());

    let mut preview: Vec<(f32, u16, f32)> = preview_note_snapshot(&editor);
    assert!(!preview.is_empty(), "预览音符不得为空");
    preview.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let history_before = undo_len(&editor);

    assert!(editor.confirm_bake_placement());

    let mut written: Vec<(f32, u16, f32)> = editor
        .editor_state
        .data
        .track_notes(1)
        .iter()
        .map(|note| {
            (
                note.start_tick as f32,
                u16::from(note.key),
                (note.end_tick - note.start_tick) as f32,
            )
        })
        .collect();
    written.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    assert_eq!(written, preview, "提交音符必须与预览逐项一致");
    assert_eq!(undo_len(&editor), history_before + 1, "✓ 应恰好一组历史");
}

#[test]
fn undo_after_confirm_restores_placement_then_second_undo_returns_to_pending() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.confirm_bake_placement());

    // 第一次撤销：回到黄色 8 手柄放置态
    assert!(editor.undo(), "应能撤销本次 Bake");
    assert!(
        editor.bake_placement_active(),
        "第一次撤销应回到 8 手柄放置态"
    );
    assert!(editor.is_shape_edit_armed(), "8 手柄应重新布防");
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 应保留音符数据"
    );

    // 第二次撤销：退出放置态，回到 pending 图形预览
    assert!(editor.undo(), "第二次撤销应退出放置态");
    assert!(!editor.bake_placement_active());
    assert!(!editor.is_shape_edit_armed(), "不应残留 8 手柄");
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "不应残留不可操作的黄色选择框"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 预览音符必须保留"
    );
}

#[test]
fn pending_confirm_enters_selection_and_undo_keeps_preview_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.confirm_bake_placement());
    assert!(editor.undo(), "第一次撤销回放置态");
    assert!(editor.undo(), "第二次撤销回 pending");
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "pending 应有预览音符"
    );

    assert!(
        editor.enter_bake_placement_from_pending(),
        "pending 再点 ✓ 应回到选择态，而不是直接 Bake"
    );
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "进入选择态不得丢预览音符"
    );

    assert!(editor.undo(), "选择态撤销应回到 pending");
    assert!(!editor.bake_placement_active());
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "选择态回到 pending 不得丢预览音符"
    );
}

#[test]
fn exiting_handle_edit_clears_placement_and_keeps_notes() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());

    // Esc / 双击退出 8 手柄：会话收口，pending 保留
    assert!(editor.exit_shape_edit());
    assert!(!editor.bake_placement_active());
    assert!(!editor.is_shape_edit_armed());
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "退出 8 手柄后不得残留黄色选择框"
    );
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "退出 8 手柄后 pending 音符预览必须保留"
    );

    // 回到 pending 再点 ✓：应重新进入选择态，而不是直接 Bake
    assert!(editor.enter_bake_placement_from_pending());
    assert!(editor.bake_placement_active());
    assert!(editor.is_shape_edit_armed());
    assert!(
        editor.editor_state.pending_composed[0].note_count() > 0,
        "重新进入选择态不得丢音符"
    );
}

#[test]
fn blank_drag_maps_pending_model_then_blank_click_restores() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let original = {
        assert!(editor.begin_shape_bake_placement());
        pending_bounds(&editor)
    };
    let original_notes = preview_note_snapshot(&editor);

    // B 阶段：拉一个更宽的新框
    editor.handle_bake_marquee_released(marquee_edit_state(10.0, 20.0, 60, 64));
    let mapped = pending_bounds(&editor);
    assert!(
        mapped.0 >= 9.9 && mapped.2 <= 20.1,
        "模型应映射到新框：{mapped:?}"
    );
    assert!(
        mapped.2 - mapped.0 > original.2 - original.0,
        "新框更宽，模型宽度应增加"
    );
    assert_ne!(
        preview_note_snapshot(&editor),
        original_notes,
        "映射后预览音符应随之重生成"
    );

    // 空白点击（无拖动）→ 复位到 Bake 前
    editor.handle_bake_marquee_released(marquee_edit_state(10.0, 10.0, 60, 60));
    assert_eq!(pending_bounds(&editor), original, "空白点击应复位");
    assert_eq!(
        preview_note_snapshot(&editor),
        original_notes,
        "复位后预览音符必须回到 Bake 前"
    );

    // 复位后仍可再次拉框（会话未失效）
    editor.handle_bake_marquee_released(marquee_edit_state(30.0, 36.0, 64, 68));
    assert!(
        pending_bounds(&editor).0 >= 29.9,
        "复位后应能再次映射新框：{:?}",
        pending_bounds(&editor)
    );
}
