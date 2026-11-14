//! 020「编辑图形」提交/撤销/旋转/中心平移测试（从 `tests.rs` 拆出）

use super::tests::{editor_with_pending_composed, enter_edit, model_bounds, screen_box};
use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::PendingShapeRef;
use lumino_ui_core::geom::Point;

/// DEC-001：Bake 后只读，提交/取消的几何变化只对 pending 组合有定义。
#[test]
fn test_pending_commit_changes_geometry() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let before = model_bounds(&editor);
    assert!(
        editor.begin_shape_edit(before.0, before.3),
        "Nw 角锚点应命中"
    );
    assert!(editor.is_shape_editing());
    assert!(editor.update_shape_edit(before.0, before.3 + 2.0));
    assert!(editor.finish_shape_edit());
    let after = model_bounds(&editor);
    assert!(
        (after.3 - (before.3 + 2.0)).abs() < 1e-3,
        "角锚点伸缩应改几何，实际 {after:?}"
    );
}

#[test]
fn test_pending_cancel_restores_geometry() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let before = editor.editor_state.pending_composed.clone();
    let bounds = model_bounds(&editor);
    assert!(editor.begin_shape_edit(bounds.0, bounds.3));
    assert!(editor.update_shape_edit(bounds.0, bounds.3 + 2.0));
    editor.cancel_shape_edit();
    assert_eq!(
        editor.editor_state.pending_composed, before,
        "取消应恢复几何"
    );
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

    enter_edit(&mut editor);
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
fn test_pending_rotate_zone_outside_corner() {
    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    let before = model_bounds(&editor);
    let model = editor.editor_state.pending_composed[0].results[0]
        .model
        .clone();
    let rect = screen_box(&editor, before);
    let rotate_screen = Point::new(rect.x + rect.width + 12.0, rect.y - 12.0);
    let tick = editor.pos_to_tick(rotate_screen);
    let key = editor.pos_to_raw_key(rotate_screen);
    assert!(editor.begin_shape_edit(tick, key), "角外 12px 应进入旋转");
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
fn test_pending_composed_handle_edits_preview_and_undo() {
    use lumino_core::shape::ShapeOp;

    let mut editor = editor_with_pending_composed();
    enter_edit(&mut editor);
    assert!(editor.begin_shape_edit(0.0, 64.0), "Nw 锚点应命中");
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
        "锚点拖动不得写文档"
    );
    assert!(
        model
            .ops
            .iter()
            .any(|op| matches!(op, ShapeOp::CopyAlong(_))),
        "CopyAlong op 应保留"
    );
    assert_eq!(editor.pending_edit_undo.len(), 1);

    assert!(editor.undo(), "pending 锚点编辑应可撤销");
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
