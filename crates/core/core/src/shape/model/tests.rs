//! 形状对象与修饰链测试

use super::*;
use crate::shape::{ShapeFill, ShapeGeometry, ShapeKind, ShapeRect, ShapeStyle};

fn sample_shape() -> ShapeModel {
    let mut shape = ShapeModel::new(7, 2, ShapeKind::Curve, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0), (480.0, 72.0)]);
    shape.geometry.closed = false;
    shape.style = ShapeStyle {
        fill: ShapeFill::Spam,
        gate_beats: 0.125,
        stroke_width: 3,
        velocity: 96,
        colour_slot: 4,
    };
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    shape.name = "wave".into();
    shape
}

#[test]
fn test_shape_model_bincode_roundtrip() {
    let shape = sample_shape();
    let bytes = bincode::serialize(&shape).expect("bincode 序列化应成功");
    let back: ShapeModel = bincode::deserialize(&bytes).expect("bincode 反序列化应成功");
    assert_eq!(back, shape);
    assert_eq!(back.ops.len(), 2, "修饰链长度应保真");
    assert_eq!(back.k, 0.25, "视图比例 k 应保真");
}

#[test]
fn test_shape_model_json_roundtrip() {
    let shape = sample_shape();
    let text = serde_json::to_string(&shape).expect("json 序列化应成功");
    let back: ShapeModel = serde_json::from_str(&text).expect("json 反序列化应成功");
    assert_eq!(back, shape);
}

#[test]
fn test_new_shape_has_empty_ops_and_default_style() {
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    assert!(shape.ops.is_empty());
    assert_eq!(shape.style, ShapeStyle::default());
    assert_eq!(shape.geometry, ShapeGeometry::default());
    assert!(!shape.hidden);
    assert!(shape.name.is_empty());
}

#[test]
fn test_builder_helpers_set_geometry_and_style() {
    let style = ShapeStyle {
        fill: ShapeFill::Fill,
        ..ShapeStyle::default()
    };
    let geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0)]);
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25)
        .with_geometry(geometry.clone())
        .with_style(style);
    assert_eq!(shape.geometry, geometry);
    assert_eq!(shape.style, style);
}

#[test]
fn test_validate_accepts_minimal_valid_shape() {
    sample_shape().validate().expect("合法形状应通过校验");
}

#[test]
fn test_validate_accepts_rect_only_shape() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 480.0, 72.0));
    shape.validate().expect("纯框形状应通过校验");
}

#[test]
fn test_validate_rejects_empty_geometry() {
    let shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    let err = shape.validate().expect_err("空几何应被拒绝");
    assert!(format!("{err}").contains("几何为空"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_bad_k() {
    for bad in [0.0_f32, -1.0, f32::NAN, f32::INFINITY] {
        let mut shape = sample_shape();
        shape.k = bad;
        let err = shape.validate().expect_err("非法视图比例应被拒绝");
        assert!(
            format!("{err}").contains("视图比例 k 非法"),
            "实际错误：{err}"
        );
    }
}

#[test]
fn test_validate_rejects_non_finite_anchor() {
    let mut shape = sample_shape();
    shape.geometry.anchors[1].pos = (f32::NAN, 64.0);
    assert!(shape.validate().is_err(), "NaN 锚点应被拒绝");
}

#[test]
fn test_validate_rejects_out_of_range_key() {
    let mut shape = sample_shape();
    shape.geometry.anchors[0].pos = (0.0, 300.0);
    let err = shape.validate().expect_err("音高越界应被拒绝");
    assert!(format!("{err}").contains("音高越界"), "实际错误：{err}");

    let mut shape = sample_shape();
    shape.geometry.anchors[0].pos = (0.0, -1.0);
    assert!(shape.validate().is_err(), "负音高应被拒绝");
}

#[test]
fn test_validate_rejects_negative_tick() {
    let mut shape = sample_shape();
    shape.geometry.anchors[2].pos = (-1.0, 72.0);
    let err = shape.validate().expect_err("负 tick 应被拒绝");
    assert!(format!("{err}").contains("tick 为负"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_unnormalized_rect() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry.rect = Some(ShapeRect {
        tick_lo: 480.0,
        key_lo: 72.0,
        tick_hi: 0.0,
        key_hi: 60.0,
    });
    let err = shape.validate().expect_err("未规范化外接框应被拒绝");
    assert!(format!("{err}").contains("未规范化"), "实际错误：{err}");
}

#[test]
fn test_validate_rejects_rect_with_out_of_range_key() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Custom, 0.25);
    shape.geometry.rect = Some(ShapeRect {
        tick_lo: 0.0,
        key_lo: 0.0,
        tick_hi: 480.0,
        key_hi: 300.0,
    });
    let err = shape.validate().expect_err("外接框音高越界应被拒绝");
    assert!(
        format!("{err}").contains("外接框音高越界"),
        "实际错误：{err}"
    );
}

#[test]
fn test_validate_rejects_bad_style() {
    let mut shape = sample_shape();
    shape.style.gate_beats = 0.0;
    assert!(shape.validate().is_err(), "零门长应被拒绝");
}

#[test]
fn test_op_chain_order_is_preserved() {
    let mut shape = ShapeModel::new(1, 0, ShapeKind::Line, 0.25);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 60.0)]);
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    shape.push_op(ShapeOp::Noop);
    assert_eq!(shape.ops.len(), 3);
    assert_eq!(ShapeOp::default(), ShapeOp::Noop);
}

#[test]
fn test_decode_garbage_returns_error_not_panic() {
    let bytes = [0xFF_u8; 8];
    assert!(
        bincode::deserialize::<ShapeModel>(&bytes).is_err(),
        "垃圾字节应解码失败而不是 panic"
    );
}
