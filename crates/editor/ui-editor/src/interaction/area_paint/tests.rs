//! DRAW-032B 区域着色交互测试（DEC-001：只对 pending 开放）

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{PendingShapeRef, shape_tool::ShapeKind as UiShapeKind};

mod baked;
mod clear;

fn rect_instance(tick0: f32, tick1: f32, key: f32) -> lumino_editor_state::ShapeInstance {
    lumino_editor_state::ShapeInstance {
        kind: UiShapeKind::Rectangle,
        rect: (tick0, key, tick1, key + 4.0),
        shift_constrained: false,
        filled: true,
        apex_high: true,
        mask: None,
    }
}

/// 一个 pending 矩形并进入区域着色，返回 `(editor, pending_composed 下标)`
fn editor_with_pending_area() -> (Editor, usize) {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(rect_instance(0.0, 100.0, 60.0));
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::Shape(0))
        .expect("pending 镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(
        editor.begin_area_paint(),
        "pending 闭合形状应能进入区域着色"
    );
    let index = match editor.area_paint.as_ref().expect("应有着色会话").target {
        AreaPaintTarget::PendingComposed { index } => index,
    };
    (editor, index)
}

fn area_count(editor: &Editor, index: usize) -> usize {
    editor.editor_state.pending_composed[index].results[0]
        .model
        .style
        .areas
        .len()
}

#[test]
fn test_paint_area_appends_spot_and_keeps_colour() {
    let (mut editor, index) = editor_with_pending_area();
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    let model = &editor.editor_state.pending_composed[index].results[0].model;
    assert_eq!(model.style.areas.len(), 1);
    assert_eq!(model.style.areas[0].colour, 3);
    assert!((model.style.areas[0].u - 0.5).abs() < 1e-4);
    assert!((model.style.areas[0].v - 0.5).abs() < 1e-4);
    assert!(
        !editor.editor_state.pending_composed[index].results[0]
            .notes
            .is_empty(),
        "pending 预览音符应重算"
    );
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "区域着色不得写文档"
    );
}

#[test]
fn test_paint_same_point_replaces_colour() {
    let (mut editor, index) = editor_with_pending_area();
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    assert!(editor.paint_area_at(50.0, 62.0, 5));
    let model = &editor.editor_state.pending_composed[index].results[0].model;
    assert_eq!(model.style.areas.len(), 1, "同一点不应累积重复 spot");
    assert_eq!(model.style.areas[0].colour, 5);
}

#[test]
fn test_paint_outside_box_does_not_edit() {
    let (mut editor, index) = editor_with_pending_area();
    assert!(!editor.paint_area_at(200.0, 62.0, 3));
    assert_eq!(area_count(&editor, index), 0);
}

#[test]
fn test_area_paint_colour_and_end() {
    let (mut editor, _) = editor_with_pending_area();
    assert!(editor.area_paint_active());
    assert_eq!(editor.area_paint_colour(), Some(1), "默认取非零色号");
    assert!(editor.set_area_paint_colour(7));
    assert_eq!(editor.area_paint_colour(), Some(7));
    assert!(!editor.set_area_paint_colour(16));
    editor.end_area_paint();
    assert!(!editor.area_paint_active());
    assert_eq!(editor.area_paint_colour(), None);
}

#[test]
fn test_selection_change_ends_area_paint() {
    let (mut editor, _) = editor_with_pending_area();
    editor.editor_state.shape_select.clear_selection();
    assert!(!editor.paint_area_at(50.0, 62.0, 3));
    assert!(!editor.area_paint_active(), "选区变化应结束着色会话");
}

#[test]
fn test_undo_restores_pending_area_paint() {
    let (mut editor, index) = editor_with_pending_area();
    assert!(editor.paint_area_at(50.0, 62.0, 3));
    assert_eq!(area_count(&editor, index), 1);
    assert!(editor.undo(), "一次着色点击应可撤销");
    assert_eq!(area_count(&editor, index), 0, "undo 应清掉 AreaSpot");
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "undo 不涉及文档音符"
    );
}

#[test]
fn test_begin_area_paint_promotes_plain_pending() {
    let (editor, _) = editor_with_pending_area();
    assert_eq!(editor.editor_state.pending_composed.len(), 1);
    assert!(
        editor.editor_state.shape_tool.shapes.is_empty(),
        "普通 pending 形状应被提升为单支组合"
    );
}

fn assert_promotion_matches_outline_preview(shift: bool, zoom: (f32, f32)) {
    use lumino_editor_state::shape_tool::effective_rect;

    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.view.zoom_x = zoom.0;
    editor.editor_state.view.zoom_y = zoom.1;
    editor
        .editor_state
        .shape_tool
        .shapes
        .push(lumino_editor_state::ShapeInstance {
            kind: UiShapeKind::Circle,
            rect: (0.0, 56.0, 480.0, 64.0),
            shift_constrained: shift,
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
    let spec = editor.editor_state.shape_tool.shapes[0].spec();
    let preview_keys: std::collections::BTreeSet<u16> = editor
        .shape_outline_blocks(spec, false)
        .iter()
        .map(|(_, _, key)| *key)
        .collect();
    assert!(
        !preview_keys.is_empty(),
        "预览应有音符行（shift={shift}, zoom={zoom:?}）"
    );

    assert!(
        editor.begin_area_paint(),
        "圆应能进入区域着色（shift={shift}, zoom={zoom:?}）"
    );
    let model = &editor.editor_state.pending_composed[0].results[0].model;
    let bounds = model.geometry.bounds().expect("几何");
    let expected = effective_rect(spec, zoom.0, zoom.1);
    assert!(
        (bounds.0 - expected.0).abs() < 1e-2
            && (bounds.1 - expected.1).abs() < 1e-2
            && (bounds.2 - expected.2).abs() < 1e-2
            && (bounds.3 - expected.3).abs() < 1e-2,
        "提升后的几何必须与当前视图尺度一致：{bounds:?} vs {expected:?}"
    );
    let note_keys: std::collections::BTreeSet<u16> = editor.editor_state.pending_composed[0]
        .results[0]
        .notes
        .iter()
        .map(|note| note.key)
        .collect();
    assert_eq!(
        note_keys, preview_keys,
        "预览音符行与提升后的按行音符必须一致（shift={shift}, zoom={zoom:?}）"
    );
}

#[test]
fn test_area_paint_promotion_matches_outline_preview() {
    for shift in [false, true] {
        for zoom in [(2.0, 20.0), (20.0, 2.0), (1.0, 1.0)] {
            assert_promotion_matches_outline_preview(shift, zoom);
        }
    }
}

#[test]
fn test_filled_circle_preview_spans_geometry_after_promotion_and_paint() {
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
    assert!(editor.begin_area_paint(), "填充圆应能进入区域着色");

    let (key_lo, key_hi) = {
        let bounds = editor.editor_state.pending_composed[0].results[0]
            .model
            .geometry
            .bounds()
            .expect("几何");
        (bounds.1.round() as i32, bounds.3.round() as i32)
    };
    let keys: (i32, i32) = {
        let notes = &editor.editor_state.pending_composed[0].results[0].notes;
        (
            notes.iter().map(|n| n.key as i32).min().unwrap_or(99),
            notes.iter().map(|n| n.key as i32).max().unwrap_or(-1),
        )
    };
    assert!(
        keys.0 <= key_lo + 1 && keys.1 >= key_hi - 1,
        "填充圆预览应覆盖几何上下边界：notes={keys:?} bounds={key_lo}..{key_hi}"
    );

    // 上色后仍应覆盖同一上下边界（不能只剩上半）
    assert!(editor.paint_area_at(240.0, 60.0, 3));
    let keys: (i32, i32) = {
        let notes = &editor.editor_state.pending_composed[0].results[0].notes;
        (
            notes.iter().map(|n| n.key as i32).min().unwrap_or(99),
            notes.iter().map(|n| n.key as i32).max().unwrap_or(-1),
        )
    };
    assert!(
        keys.0 <= key_lo + 1 && keys.1 >= key_hi - 1,
        "上色后填充音符仍应覆盖几何上下边界：notes={keys:?} bounds={key_lo}..{key_hi}"
    );
}

#[test]
fn test_switch_tool_ends_area_paint() {
    let (mut editor, _) = editor_with_pending_area();
    editor.set_tool(Tool::Pointer);
    assert!(!editor.area_paint_active());
}

#[test]
fn test_begin_area_paint_rejects_open_pending_curve() {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.line_tool.paths.push(vec![
        lumino_editor_state::BezierAnchor::new((0.0, 60.0)),
        lumino_editor_state::BezierAnchor::new((100.0, 64.0)),
    ]);
    editor.rebuild_pending_mirrors();
    let id = editor
        .editor_state
        .shape_select
        .pending_mirror(PendingShapeRef::CurvePath(0))
        .expect("pending 曲线镜像");
    editor.editor_state.shape_select.select_only(id);
    assert!(
        !editor.begin_area_paint(),
        "开放曲线没有内部区域，不得进入区域着色"
    );
    assert!(!editor.area_paint_active());
    assert!(
        editor.editor_state.pending_composed.is_empty(),
        "拒绝时不得留下提升副作用"
    );
}
