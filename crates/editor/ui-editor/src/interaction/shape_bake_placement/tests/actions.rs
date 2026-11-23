//! DRAW-049 阶段 2：动作路由与回归。
//!
//! 覆盖 `EditorAction::ShapeToolConfirm` 两段式（pending → 放置态 → 提交）、
//! 多形状回退旧提交、换工具收口、`Insert` 不被归属轨拦截，以及 i2m 放置
//! 确认仍走自己的路径。

use super::{draw_rect, marquee_edit_state, test_editor, track_note_count, undo_len};
use lumino_core::shape::{
    DrawerInsert, ShapeDrawing, ShapeGeometry, ShapeInsertSpec, ShapeKind, ShapeModel, ShapeRect,
};
use lumino_editor_state::{ComposedKind, ImageToMidiMode};
use lumino_message::Tool;
use lumino_ui_core::message::EditorAction;

#[test]
fn shape_confirm_action_enters_placement_then_commits() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    let notes_before = track_note_count(&editor, 1);
    let history_before = undo_len(&editor);

    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert!(editor.bake_placement_active(), "第一段 ✓ 应进入放置态");
    assert_eq!(
        track_note_count(&editor, 1),
        notes_before,
        "进入放置态不得写音符"
    );
    assert_eq!(undo_len(&editor), history_before, "进入放置态不得产生历史");

    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert!(!editor.bake_placement_active(), "第二段 ✓ 应提交并收口");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert!(
        track_note_count(&editor, 1) > notes_before,
        "第二段 ✓ 应写入音符"
    );
    assert_eq!(undo_len(&editor), history_before + 1, "提交应恰好一组历史");
}

#[test]
fn multi_shape_bake_falls_back_to_legacy_commit() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    editor.handle_shape_tool_pressed(10.0, 70.0, false);
    editor.handle_shape_tool_moved(14.0, 74.0);
    editor.handle_shape_tool_released();
    assert_eq!(
        editor.editor_state.shape_tool.shapes.len(),
        2,
        "应有两个待确认形状"
    );
    let notes_before = track_note_count(&editor, 1);
    let history_before = undo_len(&editor);

    editor.handle_action(EditorAction::ShapeToolConfirm);

    assert!(!editor.bake_placement_active(), "多形状不得进入放置态");
    assert!(editor.bake_placement.is_none(), "多形状不得建放置会话");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert!(
        track_note_count(&editor, 1) > notes_before,
        "多形状仍应走旧提交写音符"
    );
    assert_eq!(undo_len(&editor), history_before + 1, "旧提交恰好一组历史");
}

#[test]
fn tool_switch_keeps_pending_and_clears_session() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());

    editor.set_tool(Tool::Brush);

    assert!(!editor.bake_placement_active(), "换工具应退出放置态");
    assert!(editor.bake_placement.is_none(), "换工具应清会话");
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "换工具不得丢 pending"
    );
    assert!(editor.editor_state.pending_composed[0].note_count() > 0);

    // 回到选择工具再按 ✓：仍可在归属轨重新进入放置态
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert!(editor.bake_placement_active(), "回到选择工具后可继续放置");
}

#[test]
fn insert_pending_confirm_unchanged() {
    let mut editor = test_editor();
    let rect = ShapeRect::normalized(960.0, 60.0, 2880.0, 72.0);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Line, 0.25);
    model.geometry = ShapeGeometry::from_points([(960.0, 60.0), (2880.0, 72.0)]);
    model.validate().expect("测试形状应合法");
    let drawer = DrawerInsert::new(
        ShapeDrawing::new("case", "测试图形"),
        ShapeInsertSpec::new(4.0, 12.0, true, rect),
    );
    assert!(editor.insert_shape_pending(model, drawer));
    assert_eq!(
        editor.editor_state.pending_composed[0].kind,
        ComposedKind::Insert
    );

    let notes_before = track_note_count(&editor, 1);
    let history_before = undo_len(&editor);
    editor.handle_action(EditorAction::ShapeToolConfirm);

    // Insert 不属于归属轨语义：单支 Insert 的 ✓ 走原路径，在当前轨确认。
    assert!(!editor.bake_placement_active());
    assert!(editor.bake_placement.is_none(), "Insert 不得被放置会话接管");
    assert!(editor.editor_state.pending_composed.is_empty());
    assert!(
        track_note_count(&editor, 1) > notes_before,
        "Insert ✓ 应在当前轨写入音符"
    );
    assert_eq!(
        undo_len(&editor),
        history_before + 1,
        "Insert ✓ 恰好一组历史"
    );
}

#[test]
fn i2m_placement_unchanged() {
    let mut editor = test_editor();
    draw_rect(&mut editor);
    assert!(editor.begin_shape_bake_placement());
    editor.switch_to_track(2);
    // 非归属轨 ✓ 被拦下，且不得影响 i2m 自己的放置确认路径。
    editor.handle_action(EditorAction::ShapeToolConfirm);
    assert_eq!(editor.editor_state.pending_composed.len(), 1);

    editor.editor_state.image_to_midi.begin_selecting(0.0);
    editor.handle_i2m_released(marquee_edit_state(0.0, 8.0, 60, 68));

    assert_eq!(
        editor.editor_state.image_to_midi.mode,
        ImageToMidiMode::Placing,
        "i2m 区域确认应独立于形状工具的归属轨校验"
    );
    assert!(editor.editor_state.image_to_midi.region.is_some());
    assert_eq!(
        editor.editor_state.pending_composed.len(),
        1,
        "i2m 不得消费 pending"
    );
}
