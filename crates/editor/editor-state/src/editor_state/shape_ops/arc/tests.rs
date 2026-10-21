//! Arc 几何测试

use super::*;

#[test]
fn test_circle_through_equidistant() {
    let (a, b, c) = ((0.0, 0.0), (1.0, 1.0), (2.0, 0.0));
    let (centre, radius) = circle_through(a, b, c).expect("不共线三点应有圆");
    assert!((centre.0 - 1.0).abs() < 1e-9);
    assert!(centre.1.abs() < 1e-9);
    assert!((radius - 1.0).abs() < 1e-9);
    for p in [a, b, c] {
        assert!((dist(p, centre) - radius).abs() < 1e-6);
    }
}

#[test]
fn test_degenerate_returns_none() {
    assert!(circle_through((0.0, 0.0), (1.0, 0.0), (2.0, 0.0)).is_none());
    assert!(circle_through((0.0, 0.0), (0.0, 0.0), (2.0, 1.0)).is_none());
}

#[test]
fn test_arc_passes_middle_point_and_direction() {
    let a = (1.0, 0.0);
    let b = (0.0, -1.0);
    let c = (0.0, 1.0);
    let span = arc_span(a, b, c).expect("应有三点弧");
    assert!(
        (span.abs() - 1.5 * PI).abs() < 1e-9,
        "应走大于半圆的一侧，实际 {span}"
    );
    let points = arc_points(a, b, c);
    assert_eq!(points.first(), Some(&a));
    assert_eq!(points.last(), Some(&c));
    let nearest = points
        .iter()
        .map(|p| dist(*p, b))
        .fold(f64::INFINITY, f64::min);
    assert!(nearest < 0.05, "采样点列应经过 b，实际最近 {nearest}");
}

#[test]
fn test_arc_k_changes_roundness() {
    let a = (0.0, 0.0);
    let b = (4.0, 4.0);
    let c = (8.0, 0.0);
    let ratio = |k: f64| {
        let points = arc_points_scaled(a, b, c, k);
        let xs = points.iter().map(|p| p.0 / k).collect::<Vec<_>>();
        let ys = points.iter().map(|p| p.1).collect::<Vec<_>>();
        let w = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let h = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - ys.iter().cloned().fold(f64::INFINITY, f64::min);
        h / w
    };
    let round = ratio(1.0);
    let flat = ratio(0.25);
    assert!(round > flat * 3.0, "k 越小，屏幕空间弦越长、越扁");
}

#[test]
fn test_arc_k_transforms() {
    let rotated = rotate_k(0.5, 2.0).expect("合法 k 应可旋转");
    assert!((rotated - 8.0).abs() < 1e-12, "k' = r²/k");
    let stretched = stretch_k(0.5, 2.0, 0.5).expect("合法 k 应可拉伸");
    assert!((stretched - 2.0).abs() < 1e-12, "k' = k·sx/sy");
    assert!(rotate_k(0.0, 1.0).is_none());
    assert!(stretch_k(1.0, -1.0, 1.0).is_none());
}

#[test]
fn test_full_circle_when_start_equals_end() {
    let points = arc_points((1.0, 0.0), (-1.0, 0.0), (1.0, 0.0));
    assert!(points.len() > 200, "整圆应按 1.5° 采样");
    assert_eq!(points.first(), Some(&(1.0, 0.0)));
    assert_eq!(points.last(), Some(&(1.0, 0.0)));
}
