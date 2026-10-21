//! Polygon 几何测试

use super::*;

fn params(style: PolyStyle, points: u16) -> PolygonParams {
    PolygonParams {
        style,
        points,
        ..PolygonParams::default()
    }
}

fn shoelace(points: &[Pt]) -> f64 {
    points
        .windows(2)
        .map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1)
        .sum::<f64>()
        / 2.0
}

#[test]
fn test_square_has_flat_bottom_and_clockwise() {
    let loops = unit_loops(&params(PolyStyle::Polygon, 4));
    assert_eq!(loops.len(), 1);
    let square = &loops[0];
    assert_eq!(square.len(), 5, "四个角 + 闭合点");
    let bottom = square.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    assert_eq!(
        square
            .iter()
            .filter(|p| (p.1 - bottom).abs() < 1e-12)
            .count(),
        3,
        "底边两个角 + 闭合重复点"
    );
    assert!(shoelace(square) < 0.0, "边序必须顺时针");
}

#[test]
fn test_triangle_points_up() {
    let loops = unit_loops(&params(PolyStyle::Polygon, 3));
    let top = loops[0]
        .iter()
        .map(|p| p.1)
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((top - 1.0).abs() < 1e-12, "三角形应有唯一顶点朝上");
}

#[test]
fn test_star_alternates_outer_inner() {
    let loops = unit_loops(&PolygonParams {
        style: PolyStyle::Star,
        points: 5,
        inner: 50.0,
        ..PolygonParams::default()
    });
    let star = &loops[0];
    assert_eq!(star.len(), 11, "5 外 + 5 内 + 闭合点");
    let radii = star[..10]
        .iter()
        .map(|p| (p.0 * p.0 + p.1 * p.1).sqrt())
        .collect::<Vec<_>>();
    for pair in radii.chunks(2) {
        assert!((pair[0] - 1.0).abs() < 1e-9, "外点半径应为 1");
        assert!((pair[1] - 0.5).abs() < 1e-3, "内点半径应为 0.5");
    }
}

#[test]
fn test_cross_loops_follow_gcd() {
    let one = unit_loops(&PolygonParams {
        style: PolyStyle::Cross,
        points: 5,
        skip: 2,
        ..PolygonParams::default()
    });
    assert_eq!(one.len(), 1, "5/2 是单一五角星环");
    assert_eq!(one[0].len(), 6);

    let two = unit_loops(&PolygonParams {
        style: PolyStyle::Cross,
        points: 6,
        skip: 2,
        ..PolygonParams::default()
    });
    assert_eq!(two.len(), 2, "6/2 应拆成两个三角形环");
    assert!(two.iter().all(|loop_| loop_.len() == 4));
}

#[test]
fn test_paths_fill_box_and_keep_clockwise() {
    let rect = ShapeRect {
        tick_lo: 100.0,
        key_lo: 60.0,
        tick_hi: 500.0,
        key_hi: 68.0,
    };
    let loops = paths(&params(PolyStyle::Polygon, 4), rect);
    assert_eq!(loops.len(), 1);
    let points = &loops[0];
    let min_tick = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_tick = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_key = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_key = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    assert!((min_tick - 100.0).abs() < 1e-9);
    assert!((max_tick - 500.0).abs() < 1e-9);
    assert!((min_key - 60.0).abs() < 1e-9);
    assert!((max_key - 68.0).abs() < 1e-9);
    assert!(shoelace(points) < 0.0, "归一化后仍应顺时针");
}

#[test]
fn test_side_normals_point_outward() {
    let loops = unit_loops(&params(PolyStyle::Polygon, 4));
    let sides = side_paths(&loops[0]);
    for [a, b] in sides {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let normal = (-dy, dx); // 顺时针时左法线朝外
        let midpoint = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        assert!(
            normal.0 * midpoint.0 + normal.1 * midpoint.1 > 0.0,
            "边法线应指向 box 外侧"
        );
    }
}

#[test]
fn test_max_points_does_not_panic() {
    let loops = unit_loops(&params(PolyStyle::Polygon, 200));
    assert_eq!(loops.len(), 1);
    assert_eq!(loops[0].len(), 201);
}
