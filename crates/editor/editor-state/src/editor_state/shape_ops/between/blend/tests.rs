//! Between 插值测试

use super::*;
use lumino_core::shape::ShapeGeometry;

fn line(id: u64, y: f32, x1: f32) -> ShapeModel {
    let mut model = ShapeModel::new(id, 1, ShapeKind::Line, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, y), (x1, y)]);
    model
}

fn arc(id: u64, y: f32) -> ShapeModel {
    let mut model = ShapeModel::new(id, 1, ShapeKind::Arc, 1.0);
    model.geometry = ShapeGeometry::from_points([(0.0, y), (60.0, y + 6.0), (120.0, y)]);
    model
}

#[test]
fn test_line_midpoint_and_kind() {
    let out = blend(&line(1, 60.0, 100.0), &line(2, 64.0, 200.0), 0.5, false);
    assert_eq!(out.kind, ShapeKind::Line, "两条线仍应是线");
    assert_eq!(out.geometry.anchors.len(), 2);
    assert_eq!(out.geometry.anchors[0].pos, (0.0, 62.0));
    assert_eq!(out.geometry.anchors[1].pos, (150.0, 62.0));
}

#[test]
fn test_arc_preserves_three_points() {
    let out = blend(&arc(1, 60.0), &arc(2, 64.0), 0.5, false);
    assert_eq!(out.kind, ShapeKind::Arc);
    assert_eq!(out.geometry.anchors.len(), 3);
    assert_eq!(out.geometry.anchors[1].pos, (60.0, 68.0));
}

#[test]
fn test_rev_reverses_last_half() {
    let first = line(1, 60.0, 100.0);
    let mut last = line(2, 64.0, 200.0);
    last.geometry.anchors.reverse();
    let out = blend(&first, &last, 0.5, true);
    assert_eq!(out.geometry.anchors[0].pos, (0.0, 62.0));
    assert_eq!(out.geometry.anchors[1].pos, (150.0, 62.0));
}

#[test]
fn test_count_mismatch_resamples() {
    let out = blend(&line(1, 60.0, 100.0), &arc(2, 64.0), 0.5, false);
    assert_eq!(out.geometry.anchors.len(), 3);
    assert_eq!(out.kind, ShapeKind::Curve, "线 + 弧应降级为曲线");
}
