//! 贝塞尔锚点转换测试

use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel, ShapeRect};

use super::*;

fn shape(kind: ShapeKind, anchors: &[(f32, f32)]) -> ShapeModel {
    let mut shape = ShapeModel::new(1, 1, kind, 1.0);
    shape.geometry = ShapeGeometry::from_points(anchors.iter().copied());
    shape
}

#[test]
fn test_curve_preserves_custom_handles() {
    let mut shape = shape(
        ShapeKind::Curve,
        &[(0.0, 60.0), (60.0, 64.0), (120.0, 60.0)],
    );
    shape.geometry.anchors[0].set_out_handle((20.0, 4.0));
    shape.geometry.anchors[1].set_in_handle((-15.0, -3.0));
    shape.geometry.anchors[1].set_out_handle((15.0, 3.0));
    shape.geometry.anchors[2].set_in_handle((-20.0, -4.0));

    let (anchors, sharp) = to_bezier(&shape, 1.0);
    assert!(sharp.is_empty(), "曲线不应被标成折线尖角");
    assert_eq!(anchors, shape.geometry.anchors);
    assert_eq!(anchors[0].out_handle, (20.0, 4.0));
    assert_eq!(anchors[1].in_handle, (-15.0, -3.0));
    assert!(!anchors[1].handles_auto);
}

#[test]
fn test_polyline_marks_interior_sharp() {
    let shape = shape(
        ShapeKind::Polyline,
        &[(0.0, 60.0), (60.0, 64.0), (120.0, 60.0)],
    );
    let (anchors, sharp) = to_bezier(&shape, 1.0);
    assert_eq!(anchors.len(), 3);
    assert_eq!(sharp, vec![1]);
}

#[test]
fn test_rect_expands_to_four_corner_anchors() {
    let mut shape = ShapeModel::new(1, 1, ShapeKind::Custom, 1.0);
    shape.geometry = ShapeGeometry::from_rect(ShapeRect::normalized(0.0, 60.0, 120.0, 64.0));
    let (anchors, sharp) = to_bezier(&shape, 1.0);
    assert_eq!(anchors.len(), 4);
    assert_eq!(anchors[0].pos, (0.0, 60.0));
    assert_eq!(anchors[1].pos, (120.0, 60.0));
    assert_eq!(anchors[2].pos, (120.0, 64.0));
    assert_eq!(anchors[3].pos, (0.0, 64.0));
    assert_eq!(sharp, vec![1, 2, 3]);
}

#[test]
fn test_reverse_swaps_handles() {
    let mut anchors = [
        {
            let mut anchor = ShapeAnchor::new((0.0, 60.0));
            anchor.set_out_handle((10.0, 1.0));
            anchor
        },
        {
            let mut anchor = ShapeAnchor::new((120.0, 60.0));
            anchor.set_in_handle((-10.0, -1.0));
            anchor
        },
    ];
    reverse_anchors(&mut anchors);
    assert_eq!(anchors[0].pos, (120.0, 60.0));
    assert_eq!(anchors[0].out_handle, (-10.0, -1.0));
    assert_eq!(anchors[1].pos, (0.0, 60.0));
    assert_eq!(anchors[1].in_handle, (10.0, 1.0));
}
