//! 020 手柄 UI 测试

use super::*;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{DrawnShapeSource, PendingShapeRef, ShapeNote};

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

fn editor_with_pending_composed() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: lumino_editor_state::ShapeKind::Rectangle,
            rect: (0.0, 60.0, 100.0, 64.0),
            shift_constrained: false,
            filled: true,
            apex_high: true,
            mask: None,
        });
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("pending 镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected(), "先组合成单支 pending");
    editor
}

#[test]
fn test_pending_composed_handle_edits_preview_and_undo() {
    use lumino_core::shape::ShapeOp;

    let mut editor = editor_with_pending_composed();
    assert!(editor.enable_shape_edit(), "单支 composed 应进入手柄编辑");
    assert!(editor.begin_shape_edit(0.0, 64.0), "Nw 手柄应命中");
    assert!(editor.update_shape_edit(0.0, 68.0));
    assert!(editor.finish_shape_edit());

    let composed = &editor.editor_state.pending_composed[0];
    assert_eq!(composed.results.len(), 1);
    let model = &composed.results[0].model;
    let bounds = model.geometry.bounds().expect("几何");
    assert!((bounds.3 - 68.0).abs() < 1e-3, "pending model 应被修改");
    assert!(!composed.results[0].notes.is_empty(), "预览音符应重算");
    assert_eq!(
        editor.editor_state.data.current_track_notes().len(),
        0,
        "手柄拖动不得写文档"
    );
    assert!(
        model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::CopyAlong(_))),
        "CopyAlong op 应保留"
    );
    assert_eq!(editor.pending_edit_undo.len(), 1);

    assert!(editor.undo(), "pending 手柄编辑应可撤销");
    let restored = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("几何");
    assert!((restored.3 - 64.0).abs() < 1e-3, "undo 恢复原高度");
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);

    assert!(editor.redo(), "redo 恢复编辑后高度");
    let redone = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("几何");
    assert!((redone.3 - 68.0).abs() < 1e-3);
}
