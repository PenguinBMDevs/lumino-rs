//! Slice 任意方向切割（DRAW-025）
//!
//! `slice.rs` 的竖切只认 `x = cut`；本模块把切线与开放路径 / 音符的求交
//! 泛化到任意方向的直线。竖切保留为薄封装：
//! `cut_open_vertical(points, cut)` = `cut_open_line(points, (cut, 0), (cut, 1))`。
//!
//! 侧别约定：`side(p) = cross(b - a, p - a)`，正侧为输出第一段。调用方先用
//! [`normalize_line`] 把方向规范成「自下而上」，于是正侧恒为 tick 较小的一侧，
//! 与历史竖切（左半在前）保持一致。

use crate::ShapeNote;

use super::slice::{EPS, Pt, dedup_adjacent};

/// 路径点方向规范化：保证 `a` 在下、`b` 在上（`b.y > a.y`）
///
/// 水平线按 x 排序。方向统一后 `side > 0` 恒为 tick 较小的一侧，
/// 让「拖拽方向」不影响切出的两半顺序。
pub fn normalize_line(a: Pt, b: Pt) -> (Pt, Pt) {
    let dy = b.1 - a.1;
    let swap = if dy.abs() <= 1e-6 {
        b.0 < a.0
    } else {
        dy < 0.0
    };
    if swap { (b, a) } else { (a, b) }
}

/// 把切线沿自身方向延伸到外接框之外（两端各留 1 个单位余量）
///
/// `cut_loops` 只取线段 `a-b` 内的交点，因此拖出的短线必须先延长到
/// 覆盖整个形状；方向退化（零长度）时返回 `None`。
pub fn extend_line(a: Pt, b: Pt, bounds: (f32, f32, f32, f32)) -> Option<(Pt, Pt)> {
    let dx = f64::from(b.0 - a.0);
    let dy = f64::from(b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 <= EPS {
        return None;
    }
    let (x0, y0, x1, y1) = bounds;
    let mut t_min = f64::INFINITY;
    let mut t_max = f64::NEG_INFINITY;
    for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
        let t = (f64::from(x - a.0) * dx + f64::from(y - a.1) * dy) / len2;
        t_min = t_min.min(t);
        t_max = t_max.max(t);
    }
    if !t_min.is_finite() || !t_max.is_finite() {
        return None;
    }
    let at = |t: f64| {
        (
            (f64::from(a.0) + dx * t) as f32,
            (f64::from(a.1) + dy * t) as f32,
        )
    };
    Some((at(t_min - 1.0), at(t_max + 1.0)))
}

/// 开放折线沿直线 `a-b` 分成两侧；交点同时插入两侧
///
/// 返回 `(正侧, 负侧)`；没有交点或某一侧不足两个点时返回 `None`。
pub fn cut_open_line(points: &[Pt], a: Pt, b: Pt) -> Option<(Vec<Pt>, Vec<Pt>)> {
    if points.len() < 2 {
        return None;
    }
    let (a, b) = normalize_line(a, b);
    let dx = f64::from(b.0 - a.0);
    let dy = f64::from(b.1 - a.1);
    if dx.abs() <= EPS && dy.abs() <= EPS {
        return None;
    }
    let side = |p: Pt| dx * f64::from(p.1 - a.1) - dy * f64::from(p.0 - a.0);
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    let first = side(points[0]);
    if first >= 0.0 {
        positive.push(points[0]);
    } else {
        negative.push(points[0]);
    }
    let mut crossed = false;
    for pair in points.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        let sp = side(p);
        let sq = side(q);
        if (sp <= 0.0 && sq > 0.0) || (sp >= 0.0 && sq < 0.0) {
            let t = sp / (sp - sq);
            let point = (
                (f64::from(p.0) + f64::from(q.0 - p.0) * t) as f32,
                (f64::from(p.1) + f64::from(q.1 - p.1) * t) as f32,
            );
            positive.push(point);
            negative.push(point);
            crossed = true;
        }
        if sq >= 0.0 {
            positive.push(q);
        } else {
            negative.push(q);
        }
    }
    dedup_adjacent(&mut positive);
    dedup_adjacent(&mut negative);
    (crossed && positive.len() >= 2 && negative.len() >= 2).then_some((positive, negative))
}

/// 按直线 `a-b` 把音符分成两侧；跨切线的音符在切口处一分为二
///
/// 每个 key 行在切口上的 tick 不同（`x = a.x + (row - a.y) * dx / dy`），
/// 因此逐音符按自己所在行求切口。水平切线只按 key 行分组、不切分音符
/// （音符高度恒为 1 key，切不出半个）。
pub fn split_notes_line(notes: &[ShapeNote], a: Pt, b: Pt) -> (Vec<ShapeNote>, Vec<ShapeNote>) {
    let (a, b) = normalize_line(a, b);
    let dx = f64::from(b.0 - a.0);
    let dy = f64::from(b.1 - a.1);
    let horizontal = dy.abs() <= 1e-6;
    let mut positive = Vec::new();
    let mut negative = Vec::new();
    for note in notes {
        let row = f64::from(note.key) + 0.5;
        let cut = if horizontal {
            None
        } else {
            Some(f64::from(a.0) + (row - f64::from(a.1)) * dx / dy)
        };
        let Some(cut) = cut else {
            // 水平切线：整行归属，按行中心在线的哪一侧决定
            let side = dx * (row - f64::from(a.1));
            if side >= 0.0 {
                positive.push(*note);
            } else {
                negative.push(*note);
            }
            continue;
        };
        let start = note.tick;
        let end = note.tick + note.length.max(0.0);
        let cut = cut as f32;
        if end <= cut {
            positive.push(*note);
        } else if start >= cut {
            negative.push(*note);
        } else {
            if cut > start {
                positive.push(ShapeNote {
                    length: cut - start,
                    ..*note
                });
            }
            if end > cut {
                negative.push(ShapeNote {
                    tick: cut,
                    length: end - cut,
                    ..*note
                });
            }
        }
    }
    (positive, negative)
}

#[cfg(test)]
mod tests;
