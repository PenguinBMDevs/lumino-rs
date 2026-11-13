//! 区域着色：已 Bake 入口负向测试（DEC-001）
//!
//! 列表里的模型都是只读配方，不得进入区域着色、不得写 `AreaSpot`、
//! 不得回填音符指纹。

use super::*;
use lumino_core::shape::{ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeStyle};

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
fn test_model_list_entry_is_read_only() {
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
    let notes_before = editor.editor_state.shape_store.notes_of(model_id).to_vec();

    // DEC-001：列表里的模型都是已 Bake，不得进入区域着色。
    assert!(!editor.begin_area_paint_model(model_id));
    assert!(!editor.area_paint_active());
    assert_eq!(
        editor.editor_state.shape_store.notes_of(model_id),
        notes_before.as_slice(),
        "拒绝后 ShapeStore 音符指纹不得变化"
    );
    assert!(
        editor
            .editor_state
            .shape_store
            .model(model_id)
            .expect("模型应存在")
            .style
            .areas
            .is_empty(),
        "拒绝后不得写入 AreaSpot"
    );
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "拒绝后不得生成文档音符"
    );
}

#[test]
fn test_loaded_model_is_not_adopted_for_area_paint() {
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
    assert!(editor.create_shape(model));
    let model_id = editor.editor_state.shape_store.ids()[0];
    // 模拟载入工程：模型在，但 ShapeStore 没有本次会话音符指纹。
    editor
        .editor_state
        .shape_store
        .assign_notes(model_id, Vec::new());
    editor.editor_state.tool = Tool::ShapeSelect;
    assert!(
        !editor.begin_area_paint_model(model_id),
        "DEC-001：已 Bake 模型不得因能定位音符而被接管"
    );
    assert!(
        editor
            .editor_state
            .shape_store
            .notes_of(model_id)
            .is_empty(),
        "拒绝后不得回填 ShapeStore 音符指纹"
    );
    assert!(!editor.area_paint_active());
}

#[test]
fn test_baked_outline_model_cannot_enter_area_paint() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0), (100.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    model.geometry = geometry;
    model.style = ShapeStyle {
        fill: ShapeFill::Empty,
        ..ShapeStyle::default()
    };
    assert!(editor.create_shape(model));
    let model_id = editor.editor_state.shape_store.ids()[0];
    editor
        .editor_state
        .shape_store
        .assign_notes(model_id, Vec::new());
    editor.editor_state.tool = Tool::ShapeSelect;
    assert!(!editor.begin_area_paint_model(model_id));
    let model = editor
        .editor_state
        .shape_store
        .model(model_id)
        .expect("模型应存在");
    assert_eq!(model.style.fill, ShapeFill::Empty, "拒绝后不得切 Fill");
    assert!(model.style.areas.is_empty(), "拒绝后不得写 AreaSpot");
}

#[test]
fn test_begin_area_paint_rejects_baked_selection() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0), (100.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    model.geometry = geometry;
    let model_id = editor.editor_state.shape_store.insert(model, 0);
    let drawn_id = editor.editor_state.shape_select.add(
        1,
        None,
        lumino_editor_state::DrawnShapeSource::Polyline {
            points: vec![(0.0, 60.0), (100.0, 60.0), (100.0, 64.0), (0.0, 64.0)],
        },
        Vec::new(),
    );
    editor
        .editor_state
        .shape_select
        .set_model_id(drawn_id, Some(model_id));
    editor.editor_state.shape_select.select_only(drawn_id);
    assert!(
        !editor.begin_area_paint(),
        "DEC-001：已 Bake 选中图形不得进入区域着色"
    );
    assert!(!editor.area_paint_active());
}
