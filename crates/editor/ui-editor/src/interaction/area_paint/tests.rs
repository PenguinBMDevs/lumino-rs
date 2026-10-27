//! DRAW-032B 区域着色交互测试

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::shape::{ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeStyle};
use lumino_editor_state::{DrawnShapeSource, shape_tool::ShapeKind as UiShapeKind};

fn editor_with_shape() -> (Editor, u64, u64) {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0), (100.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    model.geometry = geometry;
    model.style = ShapeStyle {
        fill: ShapeFill::Fill,
        ..ShapeStyle::default()
    };
    let model_id = editor.editor_state.shape_store.insert(model, 0);
    let drawn_id = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: UiShapeKind::Rectangle,
            rect: (0.0, 60.0, 100.0, 64.0),
            shift_constrained: false,
            apex_high: false,
            filled: true,
        },
        Vec::new(),
    );
    editor
        .editor_state
        .shape_select
        .set_model_id(drawn_id, Some(model_id));
    editor.editor_state.shape_select.select_only(drawn_id);
    editor.editor_state.tool = Tool::ShapeSelect;
    editor.area_paint = Some(AreaPaintState {
        drawn_id: Some(drawn_id),
        model_id,
        colour: 3,
    });
    (editor, drawn_id, model_id)
}

#[test]
fn test_paint_area_appends_spot_and_keeps_colour() {
    let (mut editor, _, model_id) = editor_with_shape();
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    let model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型应存在");
    assert_eq!(model.style.areas.len(), 1);
    assert_eq!(model.style.areas[0].colour, 3);
    assert!((model.style.areas[0].u - 0.5).abs() < 1e-4);
    assert!((model.style.areas[0].v - 0.5).abs() < 1e-4);
}

#[test]
fn test_paint_same_point_replaces_colour() {
    let (mut editor, _, model_id) = editor_with_shape();
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    assert!(editor.paint_area_at(50.0, 62.0, 0));
    let model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型应存在");
    assert_eq!(model.style.areas.len(), 1, "同一点不应累积重复 spot");
    assert_eq!(model.style.areas[0].colour, 0);
}

#[test]
fn test_paint_outside_box_does_not_edit() {
    let (mut editor, _, model_id) = editor_with_shape();
    assert!(!editor.paint_area_at(200.0, 62.0, 3));
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型应存在")
            .style
            .areas
            .is_empty()
    );
}

#[test]
fn test_area_paint_colour_and_end() {
    let (mut editor, _, _) = editor_with_shape();
    assert!(editor.area_paint_active());
    assert_eq!(editor.area_paint_colour(), Some(3));
    assert!(editor.set_area_paint_colour(7));
    assert_eq!(editor.area_paint_colour(), Some(7));
    assert!(!editor.set_area_paint_colour(16));
    editor.end_area_paint();
    assert!(!editor.area_paint_active());
    assert_eq!(editor.area_paint_colour(), None);
}

#[test]
fn test_selection_change_ends_area_paint() {
    let (mut editor, _, _) = editor_with_shape();
    editor.editor_state.shape_select.clear_selection();
    assert!(!editor.paint_area_at(50.0, 62.0, 3));
    assert!(!editor.area_paint_active(), "选区变化应结束着色会话");
}

#[test]
fn test_model_list_entry_can_paint_without_drawn_shape() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.tool = Tool::ShapeSelect;
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0), (100.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    model.geometry = geometry;
    model.style = ShapeStyle {
        fill: ShapeFill::Fill,
        ..ShapeStyle::default()
    };
    let model_id = editor.editor_state.shape_store.insert(model, 0);
    editor
        .editor_state
        .shape_store
        .assign_notes(model_id, vec![shape_note()]);
    assert!(editor.begin_area_paint_model(model_id));
    assert!(editor.area_paint_active());
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    assert_eq!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型应存在")
            .style
            .areas
            .len(),
        1
    );
    let notes = editor.editor_state.data.current_track_notes();
    assert!(!notes.is_empty(), "区域着色应再生成填充音符");
    assert!(
        notes.iter().all(|note| note.channel == 3),
        "区域色应写到音符通道，实际 {:?}",
        notes.iter().map(|note| note.channel).collect::<Vec<_>>()
    );
}

fn shape_note() -> lumino_editor_state::ShapeNote {
    lumino_editor_state::ShapeNote {
        track: 1,
        tick: 0.0,
        key: 60,
        length: 100.0,
        velocity: 100,
        channel: 0,
    }
}

#[test]
fn test_switch_tool_ends_area_paint() {
    let (mut editor, _, _) = editor_with_shape();
    editor.set_tool(Tool::Pointer);
    assert!(!editor.area_paint_active());
}
