//! 区域着色：透明色（colour 0）撤销上色的回归测试
//!
//! 透明色不是「把区域标空」，而是**撤销本次上色**：
//! 轮廓图形恢复轮廓音符，填充图形恢复基础填充音符；不能把整个形状清空。

use super::*;

#[test]
fn test_paint_clear_restores_base_fill_notes() {
    let (mut editor, index) = editor_with_pending_area();
    let before = editor.editor_state.pending_composed[index].results[0]
        .notes
        .clone();
    assert!(!before.is_empty(), "填充形状应有基础填充音符");

    assert!(editor.paint_area_at(50.0, 62.0, 3));
    let colored = &editor.editor_state.pending_composed[index].results[0].notes;
    assert!(
        colored.iter().all(|note| note.channel == 3),
        "上色后填充音符应全部使用色号 3"
    );

    assert!(editor.paint_area_at(50.0, 62.0, 0));
    let model = &editor.editor_state.pending_composed[index].results[0].model;
    assert!(model.style.areas.is_empty(), "透明色应移除本次上色点");
    assert_eq!(
        model.style.fill,
        lumino_core::shape::ShapeFill::Fill,
        "原填充状态应恢复"
    );
    assert_eq!(
        editor.editor_state.pending_composed[index].results[0].notes, before,
        "透明色后应恢复原基础填充音符，而不是清空形状"
    );
}

#[test]
fn test_outline_shape_paint_then_clear_restores_outline() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: UiShapeKind::Rectangle,
            rect: (0.0, 60.0, 100.0, 64.0),
            shift_constrained: false,
            filled: false,
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
    assert!(editor.begin_area_paint(), "轮廓形状应能进入区域着色");
    let outline = editor.editor_state.pending_composed[0].results[0]
        .notes
        .clone();
    assert!(!outline.is_empty(), "轮廓形状提升后应有轮廓音符");

    assert!(editor.paint_area_at(50.0, 62.0, 3));
    assert!(
        editor.editor_state.pending_composed[0].results[0]
            .model
            .style
            .fill
            == lumino_core::shape::ShapeFill::Fill,
        "第一次上色应临时切到 Fill"
    );

    assert!(editor.paint_area_at(50.0, 62.0, 0));
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    assert_eq!(
        model.style.fill,
        lumino_core::shape::ShapeFill::Empty,
        "透明色应恢复原轮廓填充状态"
    );
    assert!(model.style.areas.is_empty());
    assert_eq!(
        editor.editor_state.pending_composed[0].results[0].notes, outline,
        "透明色后应恢复原轮廓音符，而不是把形状删掉"
    );
}

#[test]
fn test_outline_circle_paint_then_clear_restores_outline() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.view.zoom_x = 2.0;
    editor.editor_state.view.zoom_y = 20.0;
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: UiShapeKind::Circle,
            rect: (0.0, 56.0, 480.0, 64.0),
            shift_constrained: true,
            filled: false,
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
    assert!(editor.begin_area_paint(), "圆轮廓应能进入区域着色");
    let outline = editor.editor_state.pending_composed[0].results[0]
        .notes
        .clone();
    assert!(!outline.is_empty(), "圆轮廓提升后应有轮廓音符");

    assert!(editor.paint_area_at(240.0, 60.0, 3));
    assert!(editor.paint_area_at(240.0, 60.0, 0));
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    assert_eq!(
        model.style.fill,
        lumino_core::shape::ShapeFill::Empty,
        "透明色应恢复圆轮廓"
    );
    assert!(model.style.areas.is_empty());
    assert_eq!(
        editor.editor_state.pending_composed[0].results[0].notes, outline,
        "透明色后应恢复原圆轮廓音符，不得残留被截断的填充音符"
    );
}

#[test]
fn test_paint_clear_without_paint_is_noop() {
    let (mut editor, _) = editor_with_pending_area();
    let before = editor.editor_state.pending_composed[0].results[0]
        .notes
        .clone();
    assert!(
        !editor.paint_area_at(50.0, 62.0, 0),
        "没有上色时透明色不应产生撤销单元"
    );
    assert_eq!(
        editor.editor_state.pending_composed[0].results[0].notes, before,
        "无上色时透明色不得改动音符"
    );
}
