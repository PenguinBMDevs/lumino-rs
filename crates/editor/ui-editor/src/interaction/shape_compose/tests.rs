//! 019–022 UI 组合入口测试

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
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
}

#[test]
fn test_join_requires_two_shapes() {
    let mut editor = seeded_editor();
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
}

#[test]
fn test_merge_two_shapes() {
    let mut editor = seeded_editor();
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
}
