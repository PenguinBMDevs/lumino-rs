//! Between 插值曲线

use lumino_core::shape::BetweenSet;

/// 该步的插值量 0..=1；空 graph = 线性
pub fn graph_at(set: &BetweenSet, u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    if set.graph.is_empty() {
        return u;
    }
    let mut points = set.graph.clone();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if u <= points[0][0] {
        return points[0][1].clamp(0.0, 1.0);
    }
    if u >= points[points.len() - 1][0] {
        return points[points.len() - 1][1].clamp(0.0, 1.0);
    }
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a[0]..=b[0]).contains(&u) {
            let span = b[0] - a[0];
            if span.abs() < 1e-9 {
                return b[1].clamp(0.0, 1.0);
            }
            let t = (u - a[0]) / span;
            return (a[1] + (b[1] - a[1]) * t).clamp(0.0, 1.0);
        }
    }
    u
}

#[cfg(test)]
mod tests;
