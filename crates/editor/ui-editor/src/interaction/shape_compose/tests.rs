//! 019–022 UI 组合入口测试

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeOp};
use lumino_editor_state::{DrawnShapeSource, ShapeNote};

fn shape_note(tick: f32, key: u16) -> ShapeNote {
    ShapeNote {
        track: 1,
        tick,
        key,
        length: 10.0,
        velocity: 100,
        channel: 0,
    }
}

fn add_polyline(editor: &mut Editor, points: Vec<(f32, f32)>, notes: Vec<ShapeNote>) -> u64 {
    editor
        .editor_state
        .shape_select
        .add(1, None, DrawnShapeSource::Polyline { points }, notes)
}

fn seeded_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 10.0), Note::new(100.0, 64, 10.0)],
    );
    editor
}

fn empty_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor
}

#[test]
fn test_live_then_split_restores_shapes() {
    let mut editor = seeded_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 64.0), (150.0, 64.0)],
        vec![shape_note(100.0, 64)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.live_selected_shapes(), "应可折叠");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Live 折叠不应丢文档音符"
    );
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    let live_id = editor.editor_state.shape_select.selected_ids()[0];
    assert!(
        editor
            .editor_state
            .shape_store
            .model(
                editor
                    .editor_state
                    .shape_select
                    .model_id_of(live_id)
                    .expect("有模型")
            )
            .is_some()
    );
    assert_eq!(
        editor
            .editor_state
            .shape_select
            .selected_shapes()
            .next()
            .expect("选中")
            .notes
            .len(),
        2
    );

    assert!(editor.split_composed_shape(), "应可还原");
    assert_eq!(editor.editor_state.shape_select.selection_len(), 2);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "还原 Live 不应丢文档音符"
    );
}

#[test]
fn test_join_requires_two_shapes() {
    let mut editor = empty_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(a);
    assert!(!editor.join_selected_shapes(), "单形状不能 Join");

    let b = add_polyline(
        &mut editor,
        vec![(50.0, 60.0), (100.0, 60.0)],
        vec![shape_note(100.0, 60)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.join_selected_shapes());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Join 不应丢文档音符"
    );
    assert!(editor.split_composed_shape());
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "还原 Join 不应丢文档音符"
    );
}

#[test]
fn test_straighten_toggle_adds_and_removes_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (20.0, 64.0), (40.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.set_straighten(true));
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
            .any(|op| matches!(op, ShapeOp::Straighten(_)))
    );
    assert!(editor.set_straighten(false));
    assert!(
        !editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Straighten(_)))
    );
}

#[test]
fn test_symmetry_toggle_adds_and_removes_op() {
    let mut editor = seeded_editor();
    let id = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)],
        vec![shape_note(0.0, 60)],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    let model_id = editor
        .editor_state
        .shape_select
        .model_id_of(id)
        .expect("模型");
    let mut model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    model.kind = ShapeKind::Curve;
    model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    editor.editor_state.shape_store.edit(model_id, model, 0);

    assert!(editor.toggle_symmetry(true));
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)))
    );
    assert!(editor.toggle_symmetry(true));
    assert!(
        !editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::Symmetry(_)))
    );
}

#[test]
fn test_slice_vertical_makes_two_shapes() {
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
    assert!(editor.slice_selected_vertical(), "应能竖切");
    assert_eq!(editor.editor_state.shape_select.selection_len(), 2);
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
        4,
        "原音符 + 3 副本"
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
fn test_merge_two_shapes() {
    let mut editor = empty_editor();
    let a = add_polyline(
        &mut editor,
        vec![(0.0, 60.0), (50.0, 60.0)],
        vec![shape_note(0.0, 60)],
    );
    let b = add_polyline(
        &mut editor,
        vec![(100.0, 60.0), (150.0, 60.0)],
        vec![shape_note(100.0, 60)],
    );
    editor.editor_state.shape_select.select_all_of([a, b]);
    assert!(editor.merge_selected_shapes());
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        2,
        "Merge 不应丢文档音符"
    );
}
