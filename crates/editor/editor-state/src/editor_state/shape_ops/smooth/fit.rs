//! 闭环认形状（Straighten 子模块）
//!
//! 单独拆出以控制 `smooth.rs` 的文件长度；所有拟合坐标已是屏幕比例。

use super::{
    CORNER, CORNER_COST, MAX_POLYGON, Pt, SHAPE_SAMPLES, bounds, distance, point_segment_distance,
    turn_angle,
};

pub(super) fn fit_closed(points: &[Pt], corners: usize, tol: f64, size: f64) -> Option<Vec<Pt>> {
    if points.len() < 3 {
        return None;
    }
    let angle = principal_angle(points);
    let center = centroid(points);
    let rotated = rotate(points, center, -angle);
    let (min_x, max_x, min_y, max_y) = bounds(&rotated);
    let width = max_x - min_x;
    let height = max_y - min_y;
    if width < tol || height < tol {
        return None;
    }
    let half_w = width / 2.0;
    let half_h = height / 2.0;

    let rectangle = [
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
        (min_x, max_y),
    ];
    let rectangle_ok = rotated.iter().all(|p| {
        (p.1 - min_y).abs() <= tol
            || (p.1 - max_y).abs() <= tol
            || (p.0 - min_x).abs() <= tol
            || (p.0 - max_x).abs() <= tol
    });
    if corners > MAX_POLYGON {
        return None;
    }
    if corners == 4 && rectangle_ok {
        return Some(rotate(&rectangle, center, angle));
    }

    if corners >= 3 {
        for sides in 3..=MAX_POLYGON {
            if let Some(polygon) = fit_regular(sides, &rotated, center, tol) {
                // 形状越简单越优先；三角形能拟合就不试五边形
                return Some(rotate(&polygon, center, angle));
            }
        }
    }

    let circle_center = centroid(&rotated);
    let circle = rotated
        .iter()
        .map(|p| distance(*p, circle_center))
        .sum::<f64>()
        / rotated.len() as f64;
    if (half_w - half_h).abs() <= tol
        && rotated.iter().all(|p| {
            (distance(*p, circle_center) - circle).abs() <= tol + f64::from(CORNER_COST) * size
        })
    {
        let samples = circle_points(circle_center, circle, SHAPE_SAMPLES);
        return Some(rotate(&samples, center, angle));
    }

    let ellipse_ok = rotated.iter().all(|p| {
        let nx = (p.0 - center.0) / half_w;
        let ny = (p.1 - center.1) / half_h;
        let error = (nx * nx + ny * ny).sqrt() - 1.0;
        error.abs() * half_w.max(half_h) <= tol
    });
    if ellipse_ok {
        let samples = ellipse_points(center, half_w, half_h, SHAPE_SAMPLES);
        return Some(rotate(&samples, center, angle));
    }
    None
}

pub(super) fn corner_count(points: &[Pt]) -> usize {
    let n = points.len();
    if n < 3 {
        return 0;
    }
    (0..n)
        .filter(|&index| {
            let prev = points[(index + n - 1) % n];
            let current = points[index];
            let next = points[(index + 1) % n];
            turn_angle(prev, current, next) > f64::from(CORNER)
        })
        .count()
}

fn regular_polygon(sides: usize, center: Pt, radius: f64, angle: f64) -> Vec<Pt> {
    (0..sides)
        .map(|index| {
            let a = angle + std::f64::consts::TAU * index as f64 / sides as f64;
            (center.0 + radius * a.cos(), center.1 + radius * a.sin())
        })
        .collect()
}

fn circle_points(center: Pt, radius: f64, samples: usize) -> Vec<Pt> {
    (0..samples)
        .map(|index| {
            let a = std::f64::consts::TAU * index as f64 / samples as f64;
            (center.0 + radius * a.cos(), center.1 + radius * a.sin())
        })
        .collect()
}

fn ellipse_points(center: Pt, rx: f64, ry: f64, samples: usize) -> Vec<Pt> {
    (0..samples)
        .map(|index| {
            let a = std::f64::consts::TAU * index as f64 / samples as f64;
            (center.0 + rx * a.cos(), center.1 + ry * a.sin())
        })
        .collect()
}

fn fit_regular(sides: usize, points: &[Pt], center: Pt, tol: f64) -> Option<Vec<Pt>> {
    let (min_x, max_x, min_y, max_y) = bounds(points);
    let width = max_x - min_x;
    let height = max_y - min_y;
    let unit = regular_polygon(sides, (0.0, 0.0), 1.0, 0.0);
    let (u_min_x, u_max_x, u_min_y, u_max_y) = bounds(&unit);
    let u_width = u_max_x - u_min_x;
    let u_height = u_max_y - u_min_y;
    if u_width <= f64::EPSILON || u_height <= f64::EPSILON {
        return None;
    }
    let scale = (width / u_width + height / u_height) / 2.0;
    if scale <= f64::EPSILON {
        return None;
    }
    let polygon = regular_polygon(sides, center, scale, 0.0);
    near_polygon(points, &polygon, tol).then_some(polygon)
}

fn near_polygon(points: &[Pt], polygon: &[Pt], tol: f64) -> bool {
    points.iter().all(|p| {
        polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .any(|(a, b)| point_segment_distance(*p, *a, *b) <= tol)
    })
}

fn centroid(points: &[Pt]) -> Pt {
    let n = points.len().max(1) as f64;
    (
        points.iter().map(|p| p.0).sum::<f64>() / n,
        points.iter().map(|p| p.1).sum::<f64>() / n,
    )
}

fn principal_angle(points: &[Pt]) -> f64 {
    let center = centroid(points);
    let mut xx = 0.0;
    let mut yy = 0.0;
    let mut xy = 0.0;
    for p in points {
        let dx = p.0 - center.0;
        let dy = p.1 - center.1;
        xx += dx * dx;
        yy += dy * dy;
        xy += dx * dy;
    }
    0.5 * (2.0 * xy).atan2(xx - yy)
}

fn rotate(points: &[Pt], center: Pt, angle: f64) -> Vec<Pt> {
    let (sin, cos) = angle.sin_cos();
    points
        .iter()
        .map(|p| {
            let dx = p.0 - center.0;
            let dy = p.1 - center.1;
            (
                center.0 + dx * cos - dy * sin,
                center.1 + dx * sin + dy * cos,
            )
        })
        .collect()
}
