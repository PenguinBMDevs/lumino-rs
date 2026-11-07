//! 019–024：已 Bake 图形的组合/参数测试

use super::*;
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeOp};

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

    assert!(editor.toggle_symmetry(true).is_ok());
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
    assert!(editor.toggle_symmetry(true).is_ok());
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
fn test_symmetry_toggle_round_trip_keeps_geometry_and_params() {
    // DRAW-043：开 → 关 → 开 必须幂等——几何（半边锚点）任何一步都不被改写，
    // 且两次开启挂上的对称参数完全相同。
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
    let mut seed = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .clone();
    seed.kind = ShapeKind::Curve;
    seed.geometry = ShapeGeometry::from_points([(0.0, 60.0), (50.0, 64.0), (100.0, 70.0)]);
    editor.editor_state.shape_store.edit(model_id, seed, 0);

    let half = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型")
        .geometry
        .anchors
        .clone();
    let symmetry_op = |editor: &Editor| {
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .ops
            .iter()
            .find_map(|op| match op {
                ShapeOp::Symmetry(params) => Some(*params),
                _ => None,
            })
    };

    assert!(editor.toggle_symmetry(true).is_ok());
    let first = symmetry_op(&editor).expect("第一次开启应有对称 op");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .geometry
            .anchors,
        half,
        "开启对称不得改写几何"
    );

    assert!(editor.toggle_symmetry(true).is_ok());
    assert!(symmetry_op(&editor).is_none(), "再次切换应关闭对称");
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型")
            .geometry
            .anchors,
        half,
        "关闭对称后锚点必须逐点还原"
    );

    assert!(editor.toggle_symmetry(true).is_ok());
    assert_eq!(
        symmetry_op(&editor),
        Some(first),
        "重新开启应挂上与首次完全相同的对称参数"
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
