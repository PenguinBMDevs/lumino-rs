//! Drawing 多笔画再生成测试（DRAW-033A）
//!
//! 从 `tests.rs` 拆出，保持单文件长度纪律。

use super::*;
use lumino_core::shape::{
    AreaSpot, BuiltinShape, DrawingParams, ShapeAnchor, ShapeDrawing, ShapeFill, ShapeGeometry,
    ShapeKind, ShapeModel, ShapeOp, ShapeRect, Stroke, StrokeKind, StrokeRole,
};

fn drawing_model() -> ShapeModel {
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    model.geometry = ShapeGeometry {
        anchors: Vec::new(),
        rect: Some(ShapeRect::normalized(0.0, 60.0, 480.0, 64.0)),
        closed: false,
    };
    model.ops.push(ShapeOp::Drawing(DrawingParams {
        strokes: vec![
            Stroke {
                kind: StrokeKind::Polyline,
                anchors: vec![
                    ShapeAnchor::new((0.0, 60.0)),
                    ShapeAnchor::new((480.0, 60.0)),
                ],
                rect: None,
            },
            Stroke {
                kind: StrokeKind::Polyline,
                anchors: vec![
                    ShapeAnchor::new((0.0, 64.0)),
                    ShapeAnchor::new((480.0, 64.0)),
                ],
                rect: None,
            },
        ],
        roles: vec![StrokeRole::default(), StrokeRole::default()],
    }));
    model
}

#[test]
fn test_builtin_drawing_converts_and_generates_notes() {
    let mut editor = editor_with_track(1);
    editor.brush.thickness = 1;
    let drawing = BuiltinShape::Square.drawing();
    let model = drawing
        .to_model(1, 0.25, ShapeRect::normalized(0.0, 60.0, 480.0, 68.0))
        .expect("内置图形转换失败");
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty(), "内置 Square 应生成音符");
    assert!(
        notes.iter().all(|note| (60..=68).contains(&note.key)),
        "音符应落在目标框内：{:?}",
        notes.iter().map(|note| note.key).collect::<Vec<_>>()
    );
}

#[test]
fn test_drawing_multi_stroke_does_not_join_separate_strokes() {
    let mut editor = editor_with_track(1);
    editor.brush.thickness = 1;
    let notes = editor.shape_notes_of(&drawing_model(), 1);
    assert!(!notes.is_empty(), "Drawing 应生成音符");
    assert!(
        notes.iter().all(|note| note.key == 60 || note.key == 64),
        "两条分离 stroke 不得在中间 key 连成一条：{:?}",
        notes.iter().map(|note| note.key).collect::<Vec<_>>()
    );
    assert!(notes.iter().any(|note| note.key == 60));
    assert!(notes.iter().any(|note| note.key == 64));
}

#[test]
fn test_library_drawing_area_to_model_keeps_fill_and_channel() {
    let mut editor = editor_with_track(1);
    editor.brush.thickness = 1;
    let mut drawing = ShapeDrawing::new("area", "area");
    drawing.strokes.push(Stroke {
        kind: StrokeKind::Polyline,
        anchors: vec![
            ShapeAnchor::new((0.0, 0.0)),
            ShapeAnchor::new((1.0, 0.0)),
            ShapeAnchor::new((1.0, 1.0)),
            ShapeAnchor::new((0.0, 1.0)),
            ShapeAnchor::new((0.0, 0.0)),
        ],
        rect: Some(ShapeRect::normalized(0.0, 0.0, 1.0, 1.0)),
    });
    drawing.areas.push(AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 3,
    });
    drawing.roles.push(StrokeRole::default());
    let model = drawing
        .to_model(1, 0.25, ShapeRect::normalized(0.0, 60.0, 480.0, 72.0))
        .expect("转换失败");
    assert_eq!(model.style.fill, ShapeFill::Fill, "有 areas 时必须走 Fill");
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty());
    assert!(notes.iter().all(|note| note.channel == 3));
}

#[test]
fn test_drawing_single_closed_stroke_keeps_area_pipeline() {
    let mut editor = editor_with_track(1);
    editor.brush.thickness = 1;
    let mut model = closed_shape(ShapeFill::Fill, 0.25);
    model.geometry = ShapeGeometry {
        anchors: Vec::new(),
        rect: Some(ShapeRect::normalized(0.0, 60.0, 480.0, 64.0)),
        closed: false,
    };
    model.style.areas = vec![AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: 3,
    }];
    model.ops.push(ShapeOp::Drawing(DrawingParams {
        strokes: vec![Stroke {
            kind: StrokeKind::Polyline,
            anchors: vec![
                ShapeAnchor::new((0.0, 60.0)),
                ShapeAnchor::new((480.0, 60.0)),
                ShapeAnchor::new((480.0, 64.0)),
                ShapeAnchor::new((0.0, 64.0)),
                ShapeAnchor::new((0.0, 60.0)),
            ],
            rect: Some(ShapeRect::normalized(0.0, 60.0, 480.0, 64.0)),
        }],
        roles: vec![StrokeRole::default()],
    }));
    let notes = editor.shape_notes_of(&model, 1);
    assert!(!notes.is_empty(), "单闭合 Drawing 应沿用 Fill/Areas 生成");
    assert!(
        notes.iter().all(|note| note.channel == 3),
        "单闭合 Drawing 的区域色不应丢失"
    );
}
