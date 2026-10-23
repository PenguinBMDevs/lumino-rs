//! 020 手柄 UI 测试

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{DrawnShapeSource, ShapeNote};

fn editor_with_shape() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[Note::new(0.0, 60, 10.0)]);
    let id = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (40.0, 60.0)],
        },
        vec![ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 10.0,
            velocity: 100,
            channel: 0,
        }],
    );
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.ensure_model_for_drawn(id).is_some());
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor
}

#[test]
fn test_begin_update_finish_moves_handle() {
    let mut editor = editor_with_shape();
    assert!(editor.begin_shape_edit(0.0, 60.0), "角点应命中手柄");
    assert!(editor.is_shape_editing());
    assert!(editor.update_shape_edit(0.0, 64.0));
    assert!(editor.finish_shape_edit());
    let shape = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .next()
        .expect("仍选中");
    match &shape.source {
        DrawnShapeSource::Polyline { points } => {
            assert!((points[0].1 - 64.0).abs() < 1e-3, "手柄拖动应改几何");
        }
        _ => panic!("应仍是折线"),
    }
}

#[test]
fn test_cancel_restores_geometry() {
    let mut editor = editor_with_shape();
    let before = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .next()
        .expect("选中")
        .source
        .clone();
    assert!(editor.begin_shape_edit(0.0, 60.0));
    assert!(editor.update_shape_edit(0.0, 64.0));
    editor.cancel_shape_edit();
    let after = editor
        .editor_state
        .shape_select
        .selected_shapes()
        .next()
        .expect("选中")
        .source
        .clone();
    assert_eq!(before, after);
}
