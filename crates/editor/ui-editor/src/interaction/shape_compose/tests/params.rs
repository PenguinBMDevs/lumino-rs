//! 025–028：已 Bake 图形的参数操作测试

use super::*;
use lumino_core::shape::ShapeOp;
use lumino_editor_state::ShapeNote;

#[test]
fn test_slice_vertical_rejects_baked_shape() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0), (40.0, 70.0), (0.0, 70.0)],
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 40.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.editor_state.shape_select.select_only(id);
    // DEC-001：Bake 即定稿，已 Bake 形状不提供切割入口
    assert!(
        !editor.slice_selected_vertical(),
        "已 Bake 形状不应再被切分"
    );
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
}

#[test]
fn test_between_creates_steps() {
    let mut editor = seeded_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 64.0), (140.0, 64.0)],
        vec![shape_note(100.0, 64)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.create_between_selected());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 7);
}

#[test]
fn test_copy_along_adds_copies() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected());
    assert_eq!(
        editor
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .expect("选中")
            .notes
            .len(),
        5,
        "原音符 + 4 副本"
    );

    // 027：fx 页必须落盘；按模型重生成（移动形状）后副本仍在
    let new_id = editor.editor_state.shape_select.selected_ids()[0];
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(new_id)
        .expect("模型");
    let mut model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    assert!(
        model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::CopyAlong(_))),
        "CopyAlong op 应写回模型"
    );
    for anchor in &mut model.geometry.anchors {
        anchor.pos.0 += 20.0;
    }
    assert!(editor.edit_shape(model_id, model), "按模型重估应成功");
    assert_eq!(
        editor.editor_state.shape_store.notes_of(model_id).len(),
        5,
        "移动形状重生成后副本不应消失"
    );
}

#[test]
fn test_velocity_up_writes_env_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.velocity_up_selected());
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::VelocityEnv(_)))
    );
}

#[test]
fn test_velocity_up_only_changes_selected_and_undoes_once() {
    let mut editor = empty_editor();
    let selected = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60), shape_note(20.0, 61)],
    );
    let other = add_polyline(
        &mut editor,
        vec![(100.0, 64.0), (140.0, 64.0)],
        vec![shape_note(100.0, 64), shape_note(120.0, 65)],
    );
    editor.editor_state.shape_select.select_only(selected);
    let model_id = editor
        .ensure_model_for_drawn(selected)
        .expect("选中形状应有模型");
    let selected_before: Vec<u8> = editor
        .editor_state
        .shape_store
        .notes_of(model_id)
        .iter()
        .map(|note| note.velocity)
        .collect();
    let other_before: Vec<u8> = editor
        .editor_state
        .shape_select
        .shapes()
        .iter()
        .find(|shape| shape.id == other)
        .expect("非选中形状仍在")
        .notes
        .iter()
        .map(|note| note.velocity)
        .collect();

    assert!(editor.velocity_up_selected());
    let selected_after: Vec<u8> = editor
        .editor_state
        .shape_store
        .notes_of(model_id)
        .iter()
        .map(|note| note.velocity)
        .collect();
    assert_ne!(selected_after, selected_before, "选中形状应被写入力度包络");
    let other_after: Vec<u8> = editor
        .editor_state
        .shape_select
        .shapes()
        .iter()
        .find(|shape| shape.id == other)
        .expect("非选中形状仍在")
        .notes
        .iter()
        .map(|note| note.velocity)
        .collect();
    assert_eq!(other_after, other_before, "非选中形状力度必须不变");

    assert!(editor.undo(), "一次应用应只压一条历史");
    let selected_undone: Vec<u8> = editor
        .editor_state
        .shape_store
        .notes_of(model_id)
        .iter()
        .map(|note| note.velocity)
        .collect();
    assert_eq!(
        selected_undone, selected_before,
        "一次 undo 应逐音符还原力度"
    );
}
