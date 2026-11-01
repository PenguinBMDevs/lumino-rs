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
    assert!(editor.enable_shape_edit(), "进入手柄编辑模式");
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
fn test_pending_handle_points_round_trip_to_hit_test() {
    let mut editor = editor_with_pending_composed();
    assert!(editor.enable_shape_edit(), "先进入显式编辑模式");
    let points = editor.shape_edit_handle_points();
    assert!(!points.is_empty(), "单支 composed 应有手柄");
    for point in points {
        let tick = editor.pos_to_tick(point);
        let key = editor.pos_to_raw_key(point);
        assert!(
            editor.begin_shape_edit(tick, key),
            "屏幕手柄 {point:?} 应能命中"
        );
    }
}

#[test]
fn test_pending_live_handle_keeps_source_notes() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    for (tick0, tick1, key) in [(0.0, 100.0, 60), (200.0, 300.0, 64)] {
        editor
            .editor_state
            .shape_tool
            .shapes
            .push(lumino_editor_state::ShapeInstance {
                kind: lumino_editor_state::ShapeKind::Rectangle,
                rect: (tick0, key as f32, tick1, key as f32 + 4.0),
                shift_constrained: false,
                filled: true,
                apex_high: true,
                mask: None,
            });
    }
    editor.rebuild_pending_mirrors();
    let ids: Vec<u64> = (0..2)
        .filter_map(|index| {
            editor
                .editor_state
                .shape_select
                .pending_mirror(PendingShapeRef::Shape(index))
        })
        .collect();
    editor.editor_state.shape_select.select_all_of(ids);
    assert!(editor.live_selected_shapes(), "Live 组合应成功");

    let composed = &editor.editor_state.pending_composed[0];
    let notes_before = composed.note_count();
    assert!(notes_before > 0);
    let bounds = composed.results[0]
        .model
        .geometry
        .bounds()
        .expect("Live 应有展平几何");

    assert!(editor.enable_shape_edit());
    assert!(editor.begin_shape_edit(bounds.0, bounds.3));
    assert!(editor.update_shape_edit(bounds.0, bounds.3 + 4.0));
    assert!(editor.finish_shape_edit());

    let composed = &editor.editor_state.pending_composed[0];
    assert!(
        composed.note_count() > 0,
        "Live 编辑应按来源模型重算，不能按展平几何生成空/连接线音符"
    );
    assert_eq!(editor.editor_state.data.current_track_notes().len(), 0);
}

#[test]
fn test_pending_handle_edit_requires_explicit_mode() {
    let mut editor = editor_with_pending_composed();
    let bounds = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("几何");
    assert!(
        !editor.begin_shape_edit(bounds.0, bounds.3),
        "未进入编辑模式时，手柄不得抢占整组拖动"
    );
    assert!(!editor.is_shape_editing());

    editor.editor_state.shape_select.clear_selection();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Composed(0))
        .expect("composed 镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.enable_shape_edit(), "进入编辑模式");
    assert!(
        editor.begin_shape_edit(bounds.0, bounds.3),
        "进入模式后点手柄应开始编辑"
    );
    assert!(editor.is_shape_editing());
}

#[test]
fn test_pending_turn_handle_rotates_geometry() {
    let mut editor = editor_with_pending_composed();
    assert!(editor.enable_shape_edit());
    let model = editor.editor_state.pending_composed[0].results[0]
        .model
        .clone();
    let layout = edit_handles(
        &model,
        Zoom {
            x: editor.editor_state.view.zoom_x,
            y: editor.editor_state.view.zoom_y,
        },
    )
    .expect("手柄布局");
    let (_, turn_pos) = layout
        .handles
        .iter()
        .find(|(kind, _)| matches!(kind, HandleKind::Turn))
        .expect("Turn 手柄");
    let screen = editor.line_pos_screen_pos(*turn_pos);
    let tick = editor.pos_to_tick(screen);
    let key = editor.pos_to_raw_key(screen);
    assert!(editor.begin_shape_edit(tick, key), "Turn 手柄应命中");
    assert!(editor.update_shape_edit(tick + 200.0, key - 200.0));
    assert!(editor.finish_shape_edit());
    assert_ne!(
        editor.editor_state.pending_composed[0].results[0]
            .model
            .geometry,
        model.geometry,
        "旋转应改变几何"
    );
}

#[test]
fn test_pending_curve_path_handle_moves_anchor() {
    use lumino_editor_state::BezierAnchor;

    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(lumino_message::Tool::ShapeSelect);
    editor.editor_state.line_tool.paths.push(vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((100.0, 64.0)),
        BezierAnchor::new((200.0, 60.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(editor.copy_along_selected(), "曲线应能组合成单支");
    assert!(editor.enable_shape_edit(), "进入显式编辑模式");

    let before = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .anchors[0]
        .pos;
    let screen = editor.line_pos_screen_pos(before);
    let tick = editor.pos_to_tick(screen);
    let key = editor.pos_to_raw_key(screen);
    assert!(editor.begin_shape_edit(tick, key), "Path 手柄应命中");
    assert!(editor.update_shape_edit(tick, key + 4.0));
    assert!(editor.finish_shape_edit());
    let after = editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .anchors[0]
        .pos;
    assert!(
        (after.1 - (before.1 + 4.0)).abs() < 1e-3,
        "路径点应跟随拖动"
    );
}

#[test]
fn test_pending_composed_handle_edits_preview_and_undo() {
    use lumino_core::shape::ShapeOp;

    let mut editor = editor_with_pending_composed();
    assert!(editor.enable_shape_edit(), "单支 composed 应进入手柄编辑");
    assert!(editor.begin_shape_edit(0.0, 64.0), "Nw 手柄应命中");
    assert!(editor.update_shape_edit(0.0, 66.0));
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
