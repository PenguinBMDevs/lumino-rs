//! Slice 几何（DRAW-025）
//!
//! 只做几何：折线与切线求交、点是否在闭环内、闭环沿直线切成两半。
//! 片的簿记在 `sliced.rs`，饼干模在 `mould.rs`。

/// 几何容差
pub const EPS: f64 = 1e-9;

/// 路径点（tick, key）
pub type Pt = (f32, f32);

/// 多个闭环
pub type Loops = Vec<Vec<Pt>>;

/// 切成两半后的闭环集合
pub type CutHalves = (Loops, Loops);

/// 折线 `path` 与线段/无限直线 `a-b` 的交点，按路径顺序返回 `(点, t)`
///
/// `whole_line = true` 时把 `a-b` 当无限长直线，否则只取线段内。
pub fn crossings(path: &[Pt], a: Pt, b: Pt, whole_line: bool) -> Vec<(Pt, f64)> {
    let mut out: Vec<(Pt, f64)> = Vec::new();
    if path.len() < 2 {
        return out;
    }
    let p1 = to64(a);
    let p2 = to64(b);
    for index in 0..path.len() - 1 {
        let q1 = to64(path[index]);
        let q2 = to64(path[index + 1]);
        let r = (q2.0 - q1.0, q2.1 - q1.1);
        let s = (p2.0 - p1.0, p2.1 - p1.1);
        let den = cross(r, s);
        if den.abs() <= EPS {
            if cross((q1.0 - p1.0, q1.1 - p1.1), s).abs() <= EPS {
                // 共线：收集落在切线范围内的路径端点；不 panic，也不重复
                for point in [q1, q2] {
                    let u = if s.0.abs() > s.1.abs() {
                        (point.0 - p1.0) / s.0
                    } else if s.1.abs() > EPS {
                        (point.1 - p1.1) / s.1
                    } else {
                        0.0
                    };
                    if whole_line || (0.0..=1.0).contains(&u) {
                        push_unique(
                            &mut out,
                            to32(point),
                            index as f64 / (path.len() - 1) as f64,
                        );
                    }
                }
            }
            continue;
        }
        let t = cross((p1.0 - q1.0, p1.1 - q1.1), s) / den;
        let u = cross((p1.0 - q1.0, p1.1 - q1.1), r) / den;
        if !(-EPS..=1.0 + EPS).contains(&t) {
            continue;
        }
        if !whole_line && !(-EPS..=1.0 + EPS).contains(&u) {
            continue;
        }
        let point = (q1.0 + r.0 * t, q1.1 + r.1 * t);
        let order = (index as f64 + t) / (path.len() - 1) as f64;
        push_unique(&mut out, to32(point), order);
    }
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 点是否在闭环内（even-odd，多个环按奇偶叠加）
pub fn inside(loops: &[Vec<Pt>], point: Pt) -> bool {
    let mut inside = false;
    for ring in loops {
        if ring.len() < 3 {
            continue;
        }
        let mut crossings: u32 = 0;
        for index in 0..ring.len() {
            let a = ring[index];
            let b = ring[(index + 1) % ring.len()];
            let (ax, ay) = to64(a);
            let (bx, by) = to64(b);
            let py = f64::from(point.1);
            let px = f64::from(point.0);
            if (ay > py) != (by > py) {
                let x = ax + (py - ay) * (bx - ax) / (by - ay);
                if x > px {
                    crossings += 1;
                }
            }
        }
        if !crossings.is_multiple_of(2) {
            inside = !inside;
        }
    }
    inside
}

/// 闭环沿直线切成两半；两半都闭合（切口成为新边）
///
/// 当前支持单环；多环时只切第一个环，其余环按重心所在侧归入两半。
pub fn cut_loops(loops: &[Vec<Pt>], a: Pt, b: Pt) -> Option<CutHalves> {
    if loops.is_empty() {
        return None;
    }
    let first = cut_one_loop(&loops[0], a, b)?;
    let s = (f64::from(b.0 - a.0), f64::from(b.1 - a.1));
    let mut left = vec![first.0];
    let mut right = vec![first.1];
    for ring in loops.iter().skip(1) {
        let c = centroid(ring);
        let side = s.0 * f64::from(c.1 - a.1) - s.1 * f64::from(c.0 - a.0);
        if side >= 0.0 {
            left.push(ring.clone());
        } else {
            right.push(ring.clone());
        }
    }
    Some((left, right))
}

fn cut_one_loop(loop_: &[Pt], a: Pt, b: Pt) -> Option<(Vec<Pt>, Vec<Pt>)> {
    if loop_.len() < 3 {
        return None;
    }
    let p1 = to64(a);
    let p2 = to64(b);
    let s = (p2.0 - p1.0, p2.1 - p1.1);
    if s.0.abs() <= EPS && s.1.abs() <= EPS {
        return None;
    }
    let mut hits: Vec<(usize, f64, Pt)> = Vec::new();
    for index in 0..loop_.len() {
        let q1 = to64(loop_[index]);
        let q2 = to64(loop_[(index + 1) % loop_.len()]);
        let r = (q2.0 - q1.0, q2.1 - q1.1);
        let den = cross(r, s);
        if den.abs() <= EPS {
            continue;
        }
        let t = cross((p1.0 - q1.0, p1.1 - q1.1), s) / den;
        if !(-EPS..=1.0 + EPS).contains(&t) {
            continue;
        }
        let point = (q1.0 + r.0 * t, q1.1 + r.1 * t);
        if hits
            .last()
            .is_some_and(|(_, _, last)| distance(*last, to32(point)) < 1e-4)
        {
            continue;
        }
        hits.push((index, t.clamp(0.0, 1.0), to32(point)));
    }
    if hits.len() < 2 {
        return None;
    }
    let (i0, _t0, p0) = hits[0];
    let (i1, _t1, p1s) = hits[1];
    if i0 == i1 {
        return None;
    }
    let mut first = vec![p0];
    let mut index = (i0 + 1) % loop_.len();
    while index != (i1 + 1) % loop_.len() {
        first.push(loop_[index]);
        index = (index + 1) % loop_.len();
        if first.len() > loop_.len() + 2 {
            return None;
        }
    }
    first.push(p1s);
    first.push(p0);

    let mut second = vec![p1s];
    index = (i1 + 1) % loop_.len();
    while index != (i0 + 1) % loop_.len() {
        second.push(loop_[index]);
        index = (index + 1) % loop_.len();
        if second.len() > loop_.len() + 2 {
            return None;
        }
    }
    second.push(p0);
    second.push(p1s);
    if first.len() < 3 || second.len() < 3 {
        return None;
    }
    Some((first, second))
}

fn centroid(points: &[Pt]) -> Pt {
    let n = points.len().max(1) as f32;
    (
        points.iter().map(|p| p.0).sum::<f32>() / n,
        points.iter().map(|p| p.1).sum::<f32>() / n,
    )
}

fn push_unique(out: &mut Vec<(Pt, f64)>, point: Pt, t: f64) {
    if !out
        .iter()
        .any(|(p, _)| (p.0 - point.0).abs() < 1e-4 && (p.1 - point.1).abs() < 1e-4)
    {
        out.push((point, t));
    }
}

fn cross(a: (f64, f64), b: (f64, f64)) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn distance(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn to64(p: Pt) -> (f64, f64) {
    (f64::from(p.0), f64::from(p.1))
}

fn to32(p: (f64, f64)) -> Pt {
    (p.0 as f32, p.1 as f32)
}

#[cfg(test)]
mod tests;
