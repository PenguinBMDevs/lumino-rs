//! Arc 三点定圆（DRAW-014）
//!
//! 移植自 Spiderweb `notes/arc.py`：三点（起点 / 经过点 / 终点）确定一段精确圆弧。
//! 钢琴卷帘的 beat 与 key 尺度不同，因此几何在“屏幕空间”求解：
//! `x = beats / k`、`y = keys`，其中 `k` = 每 key 几拍（beats per key）。
//! 这样同一份数据在跨 zoom 时仍保持圆度；`k` 随形状落盘。

use std::f64::consts::PI;

/// 路径点（beats, key）
pub type Pt = (f64, f64);

/// 采样步长：1.5°（上游 `STEP`）
pub const ARC_STEP: f64 = PI / 120.0;

const EPS: f64 = 1e-12;

/// 三点定圆 → `(圆心, 半径)`；三点共线或两点重合返回 `None`
pub fn circle_through(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let a_sq = dist2(b, c);
    let b_sq = dist2(a, c);
    let c_sq = dist2(a, b);
    let size = a_sq.max(b_sq).max(c_sq);
    if size <= 0.0 || a_sq.min(b_sq).min(c_sq) < EPS * size {
        return None;
    }
    let s = a_sq * (b_sq + c_sq - a_sq);
    let t = b_sq * (a_sq + c_sq - b_sq);
    let u = c_sq * (a_sq + b_sq - c_sq);
    let total = s + t + u;
    if total.abs() < 1e-9 * size * size {
        return None;
    }
    let centre = (
        (s * a.0 + t * b.0 + u * c.0) / total,
        (s * a.1 + t * b.1 + u * c.1) / total,
    );
    let radius = dist(a, centre);
    (radius.is_finite() && radius > 0.0).then_some((centre, radius))
}

/// 三点弧的圆心角（弧度，带符号）；无弧返回 `None`
///
/// 符号由 `a → b → c` 的绕行方向决定：大于半圆的弧也能正确表达。
pub fn arc_span(a: Pt, b: Pt, c: Pt) -> Option<f64> {
    arc_of(a, b, c).map(|(_, _, _, span)| span)
}

/// 三点弧 → 采样点列（1.5°/点）；退化时返回折线点
///
/// 点列首尾精确落在 `a` / `c` 上（轮廓相接处不留缝）。
pub fn arc_points(a: Pt, b: Pt, c: Pt) -> Vec<Pt> {
    sample_arc(a, b, c, 1.0)
}

/// 带 `k` 的三点弧：输入/输出均为 `(beats, key)`，内部按屏幕空间求圆
pub fn arc_points_scaled(a: Pt, b: Pt, c: Pt, k: f64) -> Vec<Pt> {
    if !k.is_finite() || k <= 0.0 {
        return vec![a, b, c];
    }
    sample_arc(a, b, c, k)
}

/// `k=1` 时的圆度；`k` 非法时返回 1
pub fn arc_k_or_one(k: f64) -> f64 {
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

/// 旋转 90°：`k' = r² / k`（`r` = 当时的 beats per key）
pub fn rotate_k(k: f64, r: f64) -> Option<f64> {
    if !k.is_finite() || k <= 0.0 || !r.is_finite() || r <= 0.0 {
        return None;
    }
    Some(r * r / k)
}

/// x 拉伸 `sx`、y 拉伸 `sy`：`k' = k · sx / sy`
pub fn stretch_k(k: f64, sx: f64, sy: f64) -> Option<f64> {
    if !k.is_finite() || k <= 0.0 || !sx.is_finite() || sx <= 0.0 || !sy.is_finite() || sy <= 0.0 {
        return None;
    }
    Some(k * sx / sy)
}

fn sample_arc(a: Pt, b: Pt, c: Pt, k: f64) -> Vec<Pt> {
    let screen = |p: Pt| (p.0 / k, p.1);
    let (sa, sb, sc) = (screen(a), screen(b), screen(c));
    let Some((centre, radius, t0, span)) = arc_of(sa, sb, sc) else {
        return if b == a || b == c {
            vec![a, c]
        } else {
            vec![a, b, c]
        };
    };
    let n = ((span.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut out = Vec::with_capacity(n + 1);
    out.push(a);
    for i in 1..n {
        let t = t0 + span * i as f64 / n as f64;
        out.push((
            (centre.0 + radius * t.cos()) * k,
            centre.1 + radius * t.sin(),
        ));
    }
    out.push(c);
    out
}

fn arc_of(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64, f64, f64)> {
    if a == c && a != b {
        let centre = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let radius = dist(a, centre);
        if radius <= 0.0 || !radius.is_finite() {
            return None;
        }
        let t0 = (a.1 - centre.1).atan2(a.0 - centre.0);
        return Some((centre, radius, t0, 2.0 * PI));
    }
    let (centre, radius) = circle_through(a, b, c)?;
    let t0 = (a.1 - centre.1).atan2(a.0 - centre.0);
    let t1 = (c.1 - centre.1).atan2(c.0 - centre.0);
    let mut span = (t1 - t0).rem_euclid(2.0 * PI);
    // 选择经过 b 的那一侧：b 在 a→c 左侧时取负向（顺时针）
    if (c.0 - a.0) * (b.1 - a.1) - (c.1 - a.1) * (b.0 - a.0) > 0.0 {
        span -= 2.0 * PI;
    }
    Some((centre, radius, t0, span))
}

fn dist2(a: Pt, b: Pt) -> f64 {
    (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)
}

fn dist(a: Pt, b: Pt) -> f64 {
    dist2(a, b).sqrt()
}

#[cfg(test)]
mod tests;
