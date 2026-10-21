//! 手绘扶正测试

use super::*;

fn circle(center: Pt, radius: f64, samples: usize) -> Vec<Pt> {
    (0..samples)
        .map(|index| {
            let a = std::f64::consts::TAU * index as f64 / samples as f64;
            (center.0 + radius * a.cos(), center.1 + radius * a.sin())
        })
        .collect()
}

fn ellipse(center: Pt, rx: f64, ry: f64, samples: usize) -> Vec<Pt> {
    (0..samples)
        .map(|index| {
            let a = std::f64::consts::TAU * index as f64 / samples as f64;
            (center.0 + rx * a.cos(), center.1 + ry * a.sin())
        })
        .collect()
}

fn close(mut points: Vec<Pt>) -> Vec<Pt> {
    let first = points[0];
    points.push(first);
    points
}

#[test]
fn test_tolerance_formula_and_level_zero() {
    assert!((tolerance(100, 1.0) - 0.2).abs() < 1e-9);
    assert_eq!(tolerance(0, 10.0), 0.0);
    let input = vec![(0.0, 0.0), (1.0, 0.1), (2.0, -0.1), (3.0, 0.0)];
    assert_eq!(smooth_path(&input, 0, 1.0), input, "level=0 必须逐点还原");
}

#[test]
fn test_jittered_line_becomes_line() {
    let input = vec![
        (0.0, 0.0),
        (1.0, 0.02),
        (2.0, -0.02),
        (3.0, 0.01),
        (4.0, 0.0),
    ];
    let out = smooth_path(&input, 100, 1.0);
    assert_eq!(out[0], (0.0, 0.0));
    assert_eq!(*out.last().expect("非空"), (4.0, 0.0));
    assert!(out.iter().all(|p| p.1.abs() < 0.05), "抖动应被压平");
}

#[test]
fn test_near_circle_is_recognized() {
    let input = close(circle((50.0, 60.0), 10.0, 40));
    let out = smooth_path(&input, 100, 1.0);
    assert!(out.len() > 20, "圆应重采样为平滑点列");
    for p in &out {
        let r = distance(*p, (50.0, 60.0));
        assert!((r - 10.0).abs() < 0.2, "圆半径应稳定：{r}");
    }
}

#[test]
fn test_ellipse_keeps_aspect() {
    let input = close(ellipse((50.0, 60.0), 20.0, 10.0, 40));
    let out = smooth_path(&input, 100, 1.0);
    let (min_x, max_x, min_y, max_y) = bounds(&out);
    assert!((max_x - min_x - 40.0).abs() < 0.5);
    assert!((max_y - min_y - 20.0).abs() < 0.5);
}

#[test]
fn test_square_and_rectangle_stay_four_corners() {
    let square = close(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
    let out = smooth_path(&square, 100, 1.0);
    assert_eq!(out.len(), 5, "矩形四角 + 闭合点");
    let rect = close(vec![(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)]);
    let out = smooth_path(&rect, 100, 1.0);
    assert_eq!(out.len(), 5, "矩形不应被认成圆");
}

#[test]
fn test_triangle_recognized_but_octagon_is_not() {
    let triangle = close(
        (0..3)
            .map(|index| {
                let a = std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * index as f64 / 3.0;
                (50.0 + 10.0 * a.cos(), 60.0 + 10.0 * a.sin())
            })
            .collect(),
    );
    let out = smooth_path(&triangle, 100, 1.0);
    assert!(out.len() >= 4, "三角形应被拟合");

    let octagon = close(
        (0..8)
            .map(|index| {
                let a = std::f64::consts::TAU * index as f64 / 8.0;
                (50.0 + 10.0 * a.cos(), 60.0 + 10.0 * a.sin())
            })
            .collect(),
    );
    let out = smooth_path(&octagon, 100, 1.0);
    assert!(out.len() < 20, "超过 6 角不应重采样成圆");
}

#[test]
fn test_tilted_rectangle_keeps_tilt() {
    let angle = std::f64::consts::FRAC_PI_4;
    let (sin, cos) = angle.sin_cos();
    let rotate = |p: Pt| {
        (
            50.0 + (p.0 - 50.0) * cos - (p.1 - 60.0) * sin,
            60.0 + (p.0 - 50.0) * sin + (p.1 - 60.0) * cos,
        )
    };
    let input = close(vec![
        rotate((40.0, 55.0)),
        rotate((60.0, 55.0)),
        rotate((60.0, 65.0)),
        rotate((40.0, 65.0)),
    ]);
    let out = smooth_path(&input, 100, 1.0);
    let width = bounds(&out).1 - bounds(&out).0;
    let height = bounds(&out).3 - bounds(&out).2;
    assert!(width > 15.0 && height > 15.0, "倾斜矩形应保留面积");
}
