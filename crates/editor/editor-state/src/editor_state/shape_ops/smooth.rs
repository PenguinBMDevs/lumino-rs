//! 手绘扶正（DRAW-024）
//!
//! 只存灵敏度；每次生成时在屏幕比例下重算。`level = 0` 必须逐点等于原绘制。
//! 闭环会尝试认成圆/椭圆/矩形/正方形/正三到六边形；否则只保留直线与平滑曲线。

use lumino_core::shape::{ShapeAnchor, ShapeKind, ShapeModel};

mod fit;

use fit::{corner_count, fit_closed};

/// 路径点（逻辑坐标 tick/key）
pub type Pt = (f64, f64);

/// 短距离转向超过此角度视为角
pub const CORNER: f32 = 40.0;
/// 闭环角数超过此值不认形状
pub const MAX_POLYGON: usize = 6;
/// 终点距起点小于 size 的此比例视为闭环
pub const LOOP: f32 = 0.1;
/// 拟合形状时的采样点数
pub const SHAPE_SAMPLES: usize = 120;
/// 拟合时的采样数下限/上限
pub const PIECE_SAMPLES: (usize, usize) = (600, 6000);
/// 小于此值的笔画原样保留
pub const TOO_SMALL: f32 = 1e-3;
/// 每多一个角允许的误差增量
pub const CORNER_COST: f32 = 0.012;

/// 容差：`0.2 * size * (level / 100)^2.5`
pub fn tolerance(level: u8, size: f32) -> f32 {
    let level = f32::from(level.min(100)) / 100.0;
    0.2 * size * level.powf(2.5)
}

/// 手绘点列 → 扶正后的点列
pub fn smooth_path(points: &[Pt], level: u8, k: f32) -> Vec<Pt> {
    if level == 0 || points.len() < 3 {
        return points.to_vec();
    }
    let scale = 1.0 / f64::from(k.max(1e-6));
    let screen = points
        .iter()
        .map(|p| (p.0 * scale, p.1))
        .collect::<Vec<_>>();
    let size = diagonal(&screen);
    if size < f64::from(TOO_SMALL) {
        return points.to_vec();
    }
    let tol = f64::from(tolerance(level, size as f32).max(1e-6));
    let closed = distance(screen[0], screen[screen.len() - 1]) < f64::from(LOOP) * size;
    let work = if closed {
        screen[..screen.len() - 1].to_vec()
    } else {
        screen.clone()
    };
    let corners = if closed { corner_count(&work) } else { 0 };
    let simplified = simplify(&work, tol);
    let fitted = if closed {
        fit_closed(&work, corners, tol, size)
    } else {
        Some(smooth_open(&simplified, tol))
    }
    .unwrap_or(simplified);
    let mut out = fitted
        .into_iter()
        .map(|p| (p.0 / scale, p.1))
        .collect::<Vec<_>>();
    if closed && out.len() >= 3 {
        out.push(out[0]);
    }
    out
}

/// 形状级入口：只对手绘/折线/直线替换锚点；曲线保留原手柄
pub fn straighten_shape(model: &ShapeModel, level: u8) -> Vec<ShapeAnchor> {
    if !matches!(
        model.kind,
        ShapeKind::Freehand | ShapeKind::Polyline | ShapeKind::Line
    ) {
        return model.geometry.anchors.clone();
    }
    let points = model
        .geometry
        .anchors
        .iter()
        .map(|anchor| (f64::from(anchor.pos.0), f64::from(anchor.pos.1)))
        .collect::<Vec<_>>();
    smooth_path(&points, level, model.k)
        .into_iter()
        .map(|(x, y)| ShapeAnchor::new((x as f32, y as f32)))
        .collect()
}

fn smooth_open(points: &[Pt], tol: f64) -> Vec<Pt> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut pieces = vec![vec![points[0]]];
    for index in 1..points.len() - 1 {
        let current = pieces.last_mut().expect("至少有一个 piece");
        current.push(points[index]);
        if turn_angle(points[index - 1], points[index], points[index + 1]) > f64::from(CORNER) {
            pieces.push(vec![points[index]]);
        }
    }
    pieces
        .last_mut()
        .expect("至少有一个 piece")
        .push(points[points.len() - 1]);
    let mut out = Vec::new();
    for piece in pieces {
        let simplified = simplify(&piece, tol);
        let smoothed = if simplified.len() <= 2 || is_straight(&simplified, tol) {
            simplified
        } else {
            chaikin(&simplified, 2)
        };
        if out.is_empty() {
            out.extend(smoothed);
        } else {
            out.extend(smoothed.into_iter().skip(1));
        }
    }
    out
}

fn simplify(points: &[Pt], tol: f64) -> Vec<Pt> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    rdp(points, 0, points.len() - 1, tol, &mut keep);
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect()
}

fn rdp(points: &[Pt], start: usize, end: usize, tol: f64, keep: &mut [bool]) {
    if end <= start + 1 {
        return;
    }
    let mut best = 0.0;
    let mut index = start;
    for i in start + 1..end {
        let d = point_segment_distance(points[i], points[start], points[end]);
        if d > best {
            best = d;
            index = i;
        }
    }
    if best > tol {
        keep[index] = true;
        rdp(points, start, index, tol, keep);
        rdp(points, index, end, tol, keep);
    }
}

fn chaikin(points: &[Pt], iterations: usize) -> Vec<Pt> {
    let mut current = points.to_vec();
    for _ in 0..iterations {
        if current.len() < 2 {
            break;
        }
        let mut next = vec![current[0]];
        for pair in current.windows(2) {
            next.push((
                pair[0].0 * 0.75 + pair[1].0 * 0.25,
                pair[0].1 * 0.75 + pair[1].1 * 0.25,
            ));
            next.push((
                pair[0].0 * 0.25 + pair[1].0 * 0.75,
                pair[0].1 * 0.25 + pair[1].1 * 0.75,
            ));
        }
        next.push(current[current.len() - 1]);
        current = next;
    }
    current
}

fn is_straight(points: &[Pt], tol: f64) -> bool {
    let Some(&first) = points.first() else {
        return true;
    };
    let Some(&last) = points.last() else {
        return true;
    };
    points
        .iter()
        .all(|p| point_segment_distance(*p, first, last) <= tol)
}

pub(super) fn turn_angle(a: Pt, b: Pt, c: Pt) -> f64 {
    let v1 = (b.0 - a.0, b.1 - a.1);
    let v2 = (c.0 - b.0, c.1 - b.1);
    let n1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
    let n2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();
    if n1 < 1e-12 || n2 < 1e-12 {
        return 0.0;
    }
    let cos = ((v1.0 * v2.0 + v1.1 * v2.1) / (n1 * n2)).clamp(-1.0, 1.0);
    cos.acos().to_degrees()
}

pub(super) fn point_segment_distance(p: Pt, a: Pt, b: Pt) -> f64 {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let len2 = vx * vx + vy * vy;
    if len2 < 1e-18 {
        return distance(p, a);
    }
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0);
    distance(p, (a.0 + t * vx, a.1 + t * vy))
}

pub(super) fn distance(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn diagonal(points: &[Pt]) -> f64 {
    let (min_x, max_x, min_y, max_y) = bounds(points);
    ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt()
}

pub(super) fn bounds(points: &[Pt]) -> (f64, f64, f64, f64) {
    points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), p| {
            (
                min_x.min(p.0),
                max_x.max(p.0),
                min_y.min(p.1),
                max_y.max(p.1),
            )
        },
    )
}

#[cfg(test)]
mod tests;
