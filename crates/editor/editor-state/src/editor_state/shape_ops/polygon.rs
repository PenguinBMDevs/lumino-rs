//! Polygon / Star / Cross 几何（DRAW-015）
//!
//! 移植自 Spiderweb `notes/polygon.py`：
//! - 正多边形底边水平（三角形自然朝上）；
//! - 星形顶点朝上，外/内点交替；
//! - cross 每 `skip` 个角相连，`gcd(points, skip)` 决定环数；
//! - 所有环按顺时针，并拉伸填满形状 box（0..1 两轴）。

use std::f64::consts::PI;

use lumino_core::shape::{PolyStyle, PolygonParams, ShapeRect};

/// 路径点（tick, key）
pub type Pt = (f64, f64);

const EPS: f64 = 1e-12;

/// 参数化多边形 → 一条或多条闭合路径（按 box 归一化）
pub fn paths(params: &PolygonParams, rect: ShapeRect) -> Vec<Vec<Pt>> {
    let loops = unit_loops(params);
    let (tick_lo, tick_hi) = (
        rect.tick_lo.min(rect.tick_hi),
        rect.tick_lo.max(rect.tick_hi),
    );
    let (key_lo, key_hi) = (rect.key_lo.min(rect.key_hi), rect.key_lo.max(rect.key_hi));
    let points = loops.iter().flatten().copied().collect::<Vec<_>>();
    if points.is_empty() {
        return Vec::new();
    }
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let width = (max_x - min_x).max(EPS);
    let height = (max_y - min_y).max(EPS);
    loops
        .into_iter()
        .map(|mut loop_| {
            for point in &mut loop_ {
                let u = (point.0 - min_x) / width;
                let v = (point.1 - min_y) / height;
                *point = (
                    f64::from(tick_lo) + u * f64::from(tick_hi - tick_lo),
                    f64::from(key_lo) + v * f64::from(key_hi - key_lo),
                );
            }
            if let (Some(first), Some(last)) = (loop_.first().copied(), loop_.last().copied())
                && last != first
            {
                loop_.push(first);
            }
            loop_
        })
        .collect()
}

/// 单位圆坐标下的闭合环（y 向上，半径 1，顺时针）；测试与图案布线使用
pub fn unit_loops(params: &PolygonParams) -> Vec<Vec<Pt>> {
    let n = params.points.clamp(3, 200) as usize;
    match params.style {
        PolyStyle::Star => {
            let inner = f64::from(params.inner) / 100.0;
            let mut points = Vec::with_capacity(n * 2 + 1);
            for i in 0..n {
                for (j, radius) in [(0.0, 1.0), (0.5, inner)] {
                    let angle = PI / 2.0 - 2.0 * PI * (i as f64 + j) / n as f64;
                    points.push((radius * angle.cos(), radius * angle.sin()));
                }
            }
            points.push(points[0]);
            vec![points]
        }
        PolyStyle::Cross => {
            let skip = usize::from(params.skip).max(1) % n;
            let skip = if skip == 0 { 1 } else { skip };
            let corners = (0..n)
                .map(|i| {
                    let angle = PI / 2.0 - 2.0 * PI * i as f64 / n as f64;
                    (angle.cos(), angle.sin())
                })
                .collect::<Vec<_>>();
            let rings = gcd(n, skip);
            let mut out = Vec::with_capacity(rings);
            for first in 0..rings {
                let mut loop_ = Vec::with_capacity(n / rings + 1);
                for j in 0..(n / rings) {
                    loop_.push(corners[(first + j * skip) % n]);
                }
                loop_.push(loop_[0]);
                out.push(loop_);
            }
            out
        }
        PolyStyle::Polygon => {
            let start = -PI / 2.0 - PI / n as f64;
            let mut points = (0..n)
                .map(|i| {
                    let angle = start - 2.0 * PI * i as f64 / n as f64;
                    (angle.cos(), angle.sin())
                })
                .collect::<Vec<_>>();
            points.push(points[0]);
            vec![points]
        }
    }
}

/// 一条闭合折线的边（每段 `[start, end]`）
pub fn side_paths(points: &[Pt]) -> Vec<[Pt; 2]> {
    points.windows(2).map(|pair| [pair[0], pair[1]]).collect()
}

fn gcd(a: usize, b: usize) -> usize {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

#[cfg(test)]
mod tests;
