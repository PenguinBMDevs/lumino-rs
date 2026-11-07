//! Between 插值测试

use super::*;
use lumino_core::shape::{ShapeAnchor, ShapeGeometry};

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

fn curve_samples(anchors: &[ShapeAnchor], per_segment: usize) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for index in 0..anchors.len().saturating_sub(1) {
        let p0 = anchors[index].pos;
        let p1 = add(p0, anchors[index].out_handle);
        let p2 = add(anchors[index + 1].pos, anchors[index + 1].in_handle);
        let p3 = anchors[index + 1].pos;
        for step in 0..per_segment {
            let t = step as f32 / per_segment as f32;
            out.push(cubic(p0, p1, p2, p3, t));
        }
    }
    if let Some(last) = anchors.last() {
        out.push(last.pos);
    }
    out
}

fn point_distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[test]
fn test_resample_preserves_curve_locus() {
    let mut anchors = vec![
        ShapeAnchor::new((0.0, 0.0)),
        ShapeAnchor::new((100.0, 60.0)),
        ShapeAnchor::new((200.0, 0.0)),
    ];
    anchors[0].set_out_handle((30.0, 40.0));
    anchors[1].set_in_handle((-30.0, 40.0));
    anchors[1].set_out_handle((30.0, -40.0));
    anchors[2].set_in_handle((-30.0, -40.0));

    let resampled = resample(&anchors, 7);
    assert_eq!(resampled.len(), 7);
    let before = curve_samples(&anchors, 64);
    let after = curve_samples(&resampled, 64);
    for point in before {
        let nearest = after
            .iter()
            .map(|candidate| point_distance(point, *candidate))
            .fold(f32::INFINITY, f32::min);
        assert!(
            nearest < 1e-3,
            "补锚点后曲线采样点偏离 {nearest}（point={point:?}）"
        );
    }
}
