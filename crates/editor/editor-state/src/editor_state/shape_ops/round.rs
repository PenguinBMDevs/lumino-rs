//! 角度柔化（DRAW-044）
//!
//! 对折线路径的**内部角点**做圆角：在角点两侧各取一个切点，用一段与两边相切的
//! 圆弧替换原折角。端点（开放路径首尾）不处理。
//!
//! 坐标系：形状的 `k` 是「每 key 几拍」，因此 `屏幕 x = tick / (ppq * k)`。
//! 本模块直接吃 `(tick, key)` 点列，内部按 `k` 把 tick 折到与 key 同尺度，
//! 保证圆弧在屏幕上仍是正圆（与 `arc_points_scaled` 同口径）。
//!
//! 半径按 **key** 存储（跨 zoom 稳定）；半径过大时按相邻段长夹取，
//! 相邻角点不会互相吞，也不会自交。

use std::f64::consts::PI;

use lumino_core::shape::{RoundParams, ShapeAnchor, ShapeKind, ShapeModel};

/// 路径点（逻辑坐标 tick/key）
pub type Pt = (f64, f64);

/// 圆弧采样步长（弧度，约 7.5°）
const ARC_STEP: f64 = PI / 24.0;

/// 夹角小于此值视为共线，不处理
const MIN_ANGLE: f64 = 1e-3;
/// 夹角大于 `PI - 此值` 视为折返，不处理
const MAX_ANGLE: f64 = 1e-3;
/// 切点距离上限占相邻段长的比例（留出缝，避免相邻圆角相接）
const MAX_TANGENT_RATIO: f64 = 0.49;

/// 形状级入口：按 `RoundParams` 替换锚点
///
/// `Custom`（形状工具轮廓/库图形统一用锚点表达）参与柔化；曲线/弧的展平点与
/// 锚点下标不对应，强行按下标套用会切错位置，因此跳过。
pub fn rounded_anchors(model: &ShapeModel, params: &RoundParams, ppq: f64) -> Vec<ShapeAnchor> {
    if !matches!(
        model.kind,
        ShapeKind::Freehand | ShapeKind::Polyline | ShapeKind::Line | ShapeKind::Custom
    ) {
        return model.geometry.anchors.clone();
    }
    if params.is_empty() || model.geometry.anchors.len() < 3 {
        return model.geometry.anchors.clone();
    }
    let points = model
        .geometry
        .anchors
        .iter()
        .map(|anchor| (f64::from(anchor.pos.0), f64::from(anchor.pos.1)))
        .collect::<Vec<_>>();
    // 用冻结的 beats-per-key（创建时的视图比例），保证圆弧在屏幕上仍是正圆
    let k = f64::from(params.effective_k(model.k));
    let ppq = f64::from(params.effective_ppq(ppq.max(1.0) as u16));
    rounded_points(&points, params, ppq, k)
        .into_iter()
        .map(|(x, y)| ShapeAnchor::new((x as f32, y as f32)))
        .collect()
}

/// 折线点列 → 柔化后的点列（闭合路径自动识别首尾同点）
///
/// `points` 是 `(tick, key)`；`ppq` + `k` 把 tick 折到与 key 同尺度：
/// `x = tick / (ppq · k)`。**漏掉 ppq 会让短 tick 段的角度算成接近 180°**，
/// 圆角退化到看不见（DRAW-044 实测）。
pub fn rounded_points(points: &[Pt], params: &RoundParams, ppq: f64, k: f64) -> Vec<Pt> {
    if points.len() < 3 || params.is_empty() {
        return points.to_vec();
    }
    let scale = 1.0 / (ppq.max(1e-9) * k.max(1e-9));
    let screen: Vec<Pt> = points.iter().map(|p| (p.0 * scale, p.1)).collect();
    let len = screen.len();
    let closed = distance(screen[0], screen[len - 1]) < 1e-6;
    let count = if closed { len - 1 } else { len };
    if count < 3 {
        return points.to_vec();
    }

    let mut out: Vec<Pt> = Vec::with_capacity(len + 8);
    for index in 0..count {
        let corner = screen[index];
        let is_corner = closed || (index > 0 && index + 1 < count);
        let radius = if is_corner {
            params.radius_of(index).unwrap_or(0.0)
        } else {
            0.0
        };
        let prev = screen[(index + count - 1) % count];
        let next = screen[(index + 1) % count];
        match fillet(prev, corner, next, radius) {
            Some(piece) => out.extend(piece),
            None => out.push(corner),
        }
    }
    if closed {
        let first = out[0];
        out.push(first);
    }
    out.into_iter().map(|p| (p.0 / scale, p.1)).collect()
}

/// 角点是否可柔化：夹角必须在 `(MIN_ANGLE, PI - MAX_ANGLE)` 之间
///
/// 共线（夹角 ≈ π，例如把节点插在直线段上）与折返（夹角 ≈ 0）都没有可切的角，
/// 强行求圆会退化。单位需与几何求解一致（x 已按 beats-per-key 折到 key 尺度）。
pub fn is_roundable(prev: Pt, corner: Pt, next: Pt) -> bool {
    let r1 = (prev.0 - corner.0, prev.1 - corner.1);
    let r2 = (next.0 - corner.0, next.1 - corner.1);
    let len1 = (r1.0 * r1.0 + r1.1 * r1.1).sqrt();
    let len2 = (r2.0 * r2.0 + r2.1 * r2.1).sqrt();
    if len1 <= 1e-6 || len2 <= 1e-6 {
        return false;
    }
    let cos = ((r1.0 * r2.0 + r1.1 * r2.1) / (len1 * len2)).clamp(-1.0, 1.0);
    let theta = cos.acos();
    theta > MIN_ANGLE && theta < PI - MAX_ANGLE
}

/// 单个角点的圆角替换：返回「切点 + 弧采样 + 切点」
///
/// 退化（共线 / 折返 / 半径 0 / 段长不足）返回 `None`，调用方保留原角点。
fn fillet(prev: Pt, corner: Pt, next: Pt, radius: f32) -> Option<Vec<Pt>> {
    let radius = f64::from(radius);
    if !radius.is_finite() || radius <= 0.0 || !is_roundable(prev, corner, next) {
        return None;
    }
    let r1 = (prev.0 - corner.0, prev.1 - corner.1);
    let r2 = (next.0 - corner.0, next.1 - corner.1);
    let len1 = (r1.0 * r1.0 + r1.1 * r1.1).sqrt();
    let len2 = (r2.0 * r2.0 + r2.1 * r2.1).sqrt();
    let v1 = (r1.0 / len1, r1.1 / len1);
    let v2 = (r2.0 / len2, r2.1 / len2);
    let cos = (v1.0 * v2.0 + v1.1 * v2.1).clamp(-1.0, 1.0);
    let theta = cos.acos();
    if theta < MIN_ANGLE || theta > PI - MAX_ANGLE {
        return None;
    }
    let tan_half = (theta / 2.0).tan();
    if tan_half.abs() <= 1e-9 {
        return None;
    }
    // 切点距离 d = r / tan(θ/2)；按相邻段长夹取，避免吞掉相邻段
    let mut d = radius / tan_half;
    let limit = MAX_TANGENT_RATIO * len1.min(len2);
    if d > limit {
        d = limit;
    }
    if d <= 1e-6 {
        return None;
    }
    let r_eff = d * tan_half;
    let t1 = (corner.0 + v1.0 * d, corner.1 + v1.1 * d);
    let t2 = (corner.0 + v2.0 * d, corner.1 + v2.1 * d);
    // 圆心：沿角平分线，距角点 r / sin(θ/2)
    let bisector = (v1.0 + v2.0, v1.1 + v2.1);
    let bisector_len = (bisector.0 * bisector.0 + bisector.1 * bisector.1).sqrt();
    if bisector_len <= 1e-9 {
        return None;
    }
    let centre = (
        corner.0 + bisector.0 / bisector_len * (r_eff / (theta / 2.0).sin()),
        corner.1 + bisector.1 / bisector_len * (r_eff / (theta / 2.0).sin()),
    );
    let a1 = (t1.1 - centre.1).atan2(t1.0 - centre.0);
    let a2 = (t2.1 - centre.1).atan2(t2.0 - centre.0);
    let mut sweep = a2 - a1;
    while sweep > PI {
        sweep -= 2.0 * PI;
    }
    while sweep < -PI {
        sweep += 2.0 * PI;
    }
    let steps = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut piece = Vec::with_capacity(steps + 2);
    piece.push(t1);
    for step in 1..steps {
        let angle = a1 + sweep * step as f64 / steps as f64;
        piece.push((
            centre.0 + r_eff * angle.cos(),
            centre.1 + r_eff * angle.sin(),
        ));
    }
    piece.push(t2);
    Some(piece)
}

fn distance(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests;
