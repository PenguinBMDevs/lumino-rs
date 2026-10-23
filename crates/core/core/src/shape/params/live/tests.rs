//! Live 参数测试

use super::*;
use crate::shape::{ShapeGeometry, ShapeStyle};

fn shape(id: u64) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 1, ShapeKind::Line, 1.0);
    shape.geometry = ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0)]);
    shape.style = ShapeStyle::default();
    shape
}

fn params() -> LiveShapeParams {
    LiveShapeParams {
        strokes: vec![Stroke {
            kind: StrokeKind::Line,
            anchors: vec![
                ShapeAnchor::new((0.0, 60.0)),
                ShapeAnchor::new((120.0, 64.0)),
            ],
            rect: None,
        }],
        src: vec![1],
        from: vec![shape(1)],
    }
}

#[test]
fn test_valid_params() {
    params().validate().expect("最简 Live 参数应合法");
}

#[test]
fn test_bincode_roundtrip() {
    let params = params();
    let bytes = bincode::serialize(&params).expect("bincode 序列化失败");
    let back: LiveShapeParams = bincode::deserialize(&bytes).expect("bincode 反序列化失败");
    assert_eq!(back, params);
}

#[test]
fn test_nested_live_is_rejected() {
    let mut nested = shape(2);
    nested.ops.push(ShapeOp::Live(params()));
    let params = LiveShapeParams {
        from: vec![nested],
        ..params()
    };
    assert!(params.validate().is_err(), "from 内再含 Live 应被拒绝");
}
