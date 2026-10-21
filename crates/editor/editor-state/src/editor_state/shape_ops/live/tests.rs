//! Live 合并测试

use super::*;
use lumino_core::shape::{GlueParams, TumourParams};

fn shape(id: u64, kind: ShapeKind) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 1, kind, 1.0);
    shape.geometry = match kind {
        ShapeKind::Arc => ShapeGeometry::from_points([(0.0, 60.0), (60.0, 66.0), (120.0, 60.0)]),
        ShapeKind::Curve => ShapeGeometry::from_points([(0.0, 60.0), (60.0, 64.0), (120.0, 60.0)]),
        _ => ShapeGeometry::from_points([(0.0, 60.0), (120.0, 64.0)]),
    };
    shape
}

#[test]
fn test_three_kinds_preserved() {
    let shapes = vec![
        shape(1, ShapeKind::Line),
        shape(2, ShapeKind::Curve),
        shape(3, ShapeKind::Arc),
    ];
    let (live, warnings) = to_live(&shapes).expect("应可合并");
    assert!(warnings.is_empty());
    let params = live
        .ops
        .iter()
        .find_map(|op| match op {
            ShapeOp::Live(params) => Some(params),
            _ => None,
        })
        .expect("应有 Live 参数");
    assert_eq!(
        params.strokes.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![StrokeKind::Line, StrokeKind::Curve, StrokeKind::Arc]
    );
    assert_eq!(params.strokes[1].anchors.len(), 3, "曲线锚点数应保留");
    assert_eq!(params.strokes[2].anchors.len(), 3, "弧应保留三点");
    assert_eq!(params.src, vec![1, 2, 3]);
    assert_eq!(params.from.len(), 3);
    live.validate().expect("Live 新形状应合法");
    assert_eq!(shapes[0].geometry.anchors.len(), 2, "原形状不应被改");
}

#[test]
fn test_same_ops_kept_different_dropped_with_warning() {
    let mut a = shape(1, ShapeKind::Line);
    let mut b = shape(2, ShapeKind::Curve);
    a.ops.push(ShapeOp::Glue(GlueParams::default()));
    b.ops.push(ShapeOp::Glue(GlueParams::default()));
    let (live, warnings) = to_live(&[a.clone(), b.clone()]).expect("应可合并");
    assert!(warnings.is_empty());
    assert!(live.ops.iter().any(|op| matches!(op, ShapeOp::Glue(_))));

    let mut c = shape(3, ShapeKind::Line);
    c.ops.push(ShapeOp::Glue(GlueParams::All));
    let mut d = shape(4, ShapeKind::Line);
    d.ops
        .push(ShapeOp::Glue(GlueParams::Boxes(vec![[0.0, 0.0, 0.1, 0.1]])));
    let (live, warnings) = to_live(&[c, d]).expect("应可合并");
    assert!(!warnings.is_empty(), "设置不一致应产出 warning");
    assert!(!live.ops.iter().any(|op| matches!(op, ShapeOp::Glue(_))));
}

#[test]
fn test_tumours_dropped() {
    let mut a = shape(1, ShapeKind::Line);
    a.ops.push(ShapeOp::Tumours(TumourParams::default()));
    let (live, _) = to_live(&[a]).expect("应可合并");
    assert!(!live.ops.iter().any(|op| matches!(op, ShapeOp::Tumours(_))));
}

#[test]
fn test_split_back() {
    let shapes = vec![shape(1, ShapeKind::Line), shape(2, ShapeKind::Arc)];
    let (live, _) = to_live(&shapes).expect("应可合并");
    assert_eq!(split_back(&live, false).expect("应可还原"), shapes);
    assert!(split_back(&live, true).is_none());
}

#[test]
fn test_nested_live_rejected() {
    let (live, _) = to_live(&[shape(1, ShapeKind::Line)]).expect("应可合并");
    assert!(matches!(to_live(&[live]), Err(LiveError::TooDeep)));
}
