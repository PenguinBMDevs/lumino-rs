//! Join 测试

use super::super::bezier::to_bezier;
use super::*;
use lumino_core::shape::ShapeFill;

fn shape(id: u64, kind: ShapeKind, anchors: &[(f32, f32)]) -> ShapeModel {
    let mut shape = ShapeModel::new(id, 1, kind, 1.0);
    shape.geometry = ShapeGeometry::from_points(anchors.iter().copied());
    shape.style.fill = ShapeFill::Empty;
    shape
}

#[test]
fn test_three_kinds_and_endpoints() {
    let shapes = vec![
        shape(1, ShapeKind::Line, &[(0.0, 60.0), (100.0, 60.0)]),
        shape(
            2,
            ShapeKind::Curve,
            &[(100.0, 60.0), (150.0, 64.0), (200.0, 60.0)],
        ),
        shape(
            3,
            ShapeKind::Arc,
            &[(200.0, 60.0), (250.0, 66.0), (300.0, 60.0)],
        ),
    ];
    let joined = join(&shapes, 1.0, 1.0).expect("应可连接");
    assert_eq!(joined.segments.len(), 3);
    assert_eq!(joined.segments[2].anchors.len(), 3, "弧应保留三点");
    assert!(joined.params.gaps.is_empty(), "端点相接不应有空隙");
    assert_eq!(joined.params.src, vec![1, 2, 3]);
    assert_eq!(joined.model.kind, ShapeKind::Curve);
}

#[test]
fn test_disconnected_segment_is_gap() {
    let shapes = vec![
        shape(1, ShapeKind::Line, &[(0.0, 60.0), (100.0, 60.0)]),
        shape(2, ShapeKind::Line, &[(300.0, 60.0), (400.0, 60.0)]),
    ];
    let joined = join(&shapes, 1.0, 1.0).expect("应可连接");
    assert_eq!(joined.params.gaps, vec![1], "第二段应记录为空隙");
}

#[test]
fn test_split_here_and_separate() {
    let shapes = vec![
        shape(1, ShapeKind::Line, &[(0.0, 60.0), (100.0, 60.0)]),
        shape(2, ShapeKind::Line, &[(100.0, 60.0), (200.0, 60.0)]),
    ];
    let joined = join(&shapes, 1.0, 1.0).expect("应可连接");
    let original = joined.model.geometry.anchors.clone();
    let (left, right) = split_here(&joined, 1);
    let mut merged = left.model.geometry.anchors.clone();
    merged.extend(right.model.geometry.anchors.iter().skip(1).copied());
    assert_eq!(merged, original, "两半合起来应等于原点列");

    assert_eq!(split_separate(&joined).expect("应可还原"), shapes);
    let mut changed = joined.clone();
    changed.model.geometry.anchors[0].pos.0 += 10.0;
    assert!(split_separate(&changed).is_none());
}

#[test]
fn test_join_reverse_swaps_handles() {
    let mut second = shape(2, ShapeKind::Curve, &[(200.0, 60.0), (100.0, 60.0)]);
    second.geometry.anchors[0].set_out_handle((5.0, 1.0));
    second.geometry.anchors[1].set_in_handle((-5.0, -1.0));
    let joined = join(
        &[
            shape(1, ShapeKind::Line, &[(0.0, 60.0), (100.0, 60.0)]),
            second,
        ],
        1.0,
        1.0,
    )
    .expect("应可连接");
    let reversed = &joined.segments[1].anchors;
    assert_eq!(reversed[0].pos, (100.0, 60.0), "第二段应反向接入");
    assert_eq!(
        reversed[0].out_handle,
        (-5.0, -1.0),
        "反向时入/出控制柄必须交换"
    );
    assert_eq!(reversed[1].in_handle, (5.0, 1.0));
}

#[test]
fn test_to_bezier_keeps_arc_points() {
    let arc = shape(
        1,
        ShapeKind::Arc,
        &[(0.0, 60.0), (60.0, 66.0), (120.0, 60.0)],
    );
    let (points, sharp) = to_bezier(&arc, 1.0);
    assert_eq!(points.len(), 3);
    assert!(sharp.is_empty());
}
