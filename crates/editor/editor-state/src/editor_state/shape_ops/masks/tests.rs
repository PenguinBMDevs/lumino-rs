//! Mask 形状接线测试

use lumino_core::shape::{
    MaskParams, ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect, ShapeStyle,
};

use super::*;
use crate::ShapeNote;

fn note(tick: f32, key: u16, length: f32) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity: 100,
    }
}

fn shape_with_polygon() -> ShapeModel {
    let mut geometry =
        ShapeGeometry::from_points([(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)]);
    geometry.closed = true;
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25).with_geometry(geometry);
    model.push_op(ShapeOp::Mask(MaskParams::default()));
    model
}

#[test]
fn test_params_and_polygon_of_mask_shape() {
    let model = shape_with_polygon();
    assert!(params_of(&model).is_some());
    assert!(!is_solid(&model));
    let polygon = polygon_of(&model).expect("闭合锚点应给出轮廓");
    assert_eq!(polygon.len(), 4);
}

#[test]
fn test_rect_fallback_and_solid_detection() {
    let mut model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25).with_geometry(
        ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 480.0, 64.0)),
    );
    model.style = ShapeStyle {
        fill: ShapeFill::Spam,
        ..ShapeStyle::default()
    };
    model.push_op(ShapeOp::Mask(MaskParams::default()));
    assert!(is_solid(&model));
    let polygon = polygon_of(&model).expect("rect 应兜底为矩形轮廓");
    assert_eq!(polygon.len(), 4);
}

#[test]
fn test_member_of_builds_inside_area() {
    let model = shape_with_polygon();
    let notes = vec![note(100.0, 61, 100.0)];
    let member = member_of(&model, &notes, 480.0, 128).expect("应构造成员");
    assert!(!member.base.is_empty());
    assert_eq!(member.own.spans_on(61).len(), 1);
}

#[test]
fn test_no_mask_op_returns_none() {
    let model = ShapeModel::new(0, 1, ShapeKind::Custom, 0.25);
    assert!(params_of(&model).is_none());
    assert!(member_of(&model, &[], 480.0, 128).is_none());
}
