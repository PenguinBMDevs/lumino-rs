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

fn segment_controls(anchors: &[ShapeAnchor], index: usize) -> [(f32, f32); 4] {
    let p0 = anchors[index].pos;
    let p1 = add(p0, anchors[index].out_handle);
    let p2 = add(anchors[index + 1].pos, anchors[index + 1].in_handle);
    let p3 = anchors[index + 1].pos;
    [p0, p1, p2, p3]
}

#[test]
fn test_resample_preserves_curve_locus() {
    let mut anchors = vec![
        ShapeAnchor::new((0.0, 0.0)),
        ShapeAnchor::new((0.5, 0.6)),
        ShapeAnchor::new((1.0, 0.0)),
    ];
    anchors[0].set_out_handle((0.15, 0.2));
    anchors[1].set_in_handle((-0.15, 0.2));
    anchors[1].set_out_handle((0.15, -0.2));
    anchors[2].set_in_handle((-0.15, -0.2));

    let resampled = resample(&anchors, 7);
    assert_eq!(resampled.len(), 7);
    let original_segments = anchors.len() - 1;
    let parts = (resampled.len() - 1) / original_segments;
    assert_eq!((resampled.len() - 1) % original_segments, 0);
    let per_part = 16;
    for segment in 0..original_segments {
        let [a0, a1, a2, a3] = segment_controls(&anchors, segment);
        for part in 0..parts {
            let [b0, b1, b2, b3] = segment_controls(&resampled, segment * parts + part);
            for step in 0..=per_part {
                let local = step as f32 / per_part as f32;
                let original_u = (part as f32 + local) / parts as f32;
                let before = cubic(a0, a1, a2, a3, original_u);
                let after = cubic(b0, b1, b2, b3, local);
                let distance = ((before.0 - after.0).powi(2) + (before.1 - after.1).powi(2)).sqrt();
                assert!(
                    distance < 1e-6,
                    "补锚点后曲线采样点偏离 {distance}（segment={segment} part={part} step={step}）"
                );
            }
        }
    }
}
