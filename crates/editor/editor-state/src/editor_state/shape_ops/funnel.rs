//! Funnel 最简几何（DRAW-018）
//!
//! 斜 box：`P(u, f) = S + u·U + f·V`，其中 `S+U` 是线与墙的交点，
//! `S+U+V` 是一个墙端。每个墙端一个 box；两个墙端覆盖线/墙夹出的区域。
//! 本卡端曲线按直线处理，`starts` 只改变 S 的位置。

use lumino_core::shape::FunnelParams;

use super::masks::RowSpan;

/// 路径点（tick, key）
pub type Pt = (f64, f64);

const EPS: f64 = 1e-9;

/// 斜 box：`point(u, f) = s + u*u_vec + f*v_vec`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlantBox {
    /// 起点（线上）
    pub s: Pt,
    /// 沿线方向（到墙交点）
    pub u: Pt,
    /// 沿墙方向（交点到墙端）
    pub v: Pt,
}

impl SlantBox {
    /// 参数点
    pub fn point(&self, u: f64, f: f64) -> Pt {
        (
            self.s.0 + u * self.u.0 + f * self.v.0,
            self.s.1 + u * self.u.1 + f * self.v.1,
        )
    }

    /// 反解 `(u, f)`；box 退化时 `None`
    pub fn inverse(&self, point: Pt) -> Option<(f64, f64)> {
        let det = self.u.0 * self.v.1 - self.u.1 * self.v.0;
        if det.abs() < EPS {
            return None;
        }
        let dx = point.0 - self.s.0;
        let dy = point.1 - self.s.1;
        let u = (dx * self.v.1 - dy * self.v.0) / det;
        let f = (self.u.0 * dy - self.u.1 * dx) / det;
        Some((u, f))
    }
}

/// 构造所有斜 box（每个墙端一个；starts 为空时用线起点）
pub fn slant_boxes(params: &FunnelParams) -> Vec<SlantBox> {
    let Some(line) = params.lines.first() else {
        return Vec::new();
    };
    let start = (f64::from(line[0][0]), f64::from(line[0][1]));
    let end = (f64::from(line[1][0]), f64::from(line[1][1]));
    let wall = [
        (f64::from(params.wall[0][0]), f64::from(params.wall[0][1])),
        (f64::from(params.wall[1][0]), f64::from(params.wall[1][1])),
    ];
    let Some(cross) = segment_intersection(start, end, wall[0], wall[1]) else {
        return Vec::new();
    };
    let starts: Vec<Pt> = if params.starts.is_empty() {
        vec![start]
    } else {
        params
            .starts
            .iter()
            .map(|item| {
                let t = f64::from(item.at.clamp(0.0, 1.0));
                (
                    start.0 + (end.0 - start.0) * t,
                    start.1 + (end.1 - start.1) * t,
                )
            })
            .collect()
    };
    let mut out = Vec::new();
    for s in starts {
        for wall_end in wall {
            if (wall_end.0 - cross.0).hypot(wall_end.1 - cross.1) < EPS {
                continue;
            }
            out.push(SlantBox {
                s,
                u: (cross.0 - s.0, cross.1 - s.1),
                v: (wall_end.0 - cross.0, wall_end.1 - cross.1),
            });
        }
    }
    out
}

/// 斜 box 的逐 key 行 span；`gate > 0` 时按门长切开（Spam）
pub fn funnel_rows(box_: &SlantBox, gate: f32, _ppq: f64) -> Vec<RowSpan> {
    let points = [
        box_.point(0.0, 0.0),
        box_.point(1.0, 0.0),
        box_.point(1.0, 1.0),
        box_.point(0.0, 1.0),
    ];
    let min_key = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_key = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    if !min_key.is_finite() || !max_key.is_finite() {
        return Vec::new();
    }
    let gate = if gate.is_finite() && gate >= 1.0 {
        gate.round() as f64
    } else {
        0.0
    };
    let mut out = Vec::new();
    for key in min_key.floor() as i64..=max_key.ceil() as i64 {
        if !(0..=255).contains(&key) {
            continue;
        }
        let Some((start, end)) = row_interval(&points, key as f64) else {
            continue;
        };
        let start = start.max(0.0);
        if end <= start {
            continue;
        }
        if gate <= 0.0 {
            out.push(RowSpan {
                key: key as u16,
                start: start as f32,
                end: end as f32,
            });
            continue;
        }
        let mut t = start;
        while t < end - EPS {
            let next = (t + gate).min(end);
            if next > t {
                out.push(RowSpan {
                    key: key as u16,
                    start: t as f32,
                    end: next as f32,
                });
            }
            t = next;
        }
    }
    out
}

/// 斜 box 的边界折线（Empty 描边用；闭合平行四边形）
pub fn box_outline(box_: &SlantBox) -> Vec<Pt> {
    let points = vec![
        box_.point(0.0, 0.0),
        box_.point(1.0, 0.0),
        box_.point(1.0, 1.0),
        box_.point(0.0, 1.0),
        box_.point(0.0, 0.0),
    ];
    points
}

fn row_interval(points: &[Pt; 4], key: f64) -> Option<(f64, f64)> {
    let mut xs = Vec::with_capacity(4);
    for index in 0..4 {
        let a = points[index];
        let b = points[(index + 1) % 4];
        if (a.1 - key).abs() < EPS && (b.1 - key).abs() < EPS {
            xs.push(a.0);
            xs.push(b.0);
        } else if (a.1 - key).abs() < EPS {
            xs.push(a.0);
        } else if (b.1 - key).abs() < EPS {
            xs.push(b.0);
        } else if (a.1 - key) * (b.1 - key) < 0.0 {
            let t = (key - a.1) / (b.1 - a.1);
            xs.push(a.0 + (b.0 - a.0) * t);
        }
    }
    if xs.len() < 2 {
        return None;
    }
    let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (max - min > EPS).then_some((min, max))
}

/// 两条线段求交（含端点）
fn segment_intersection(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let denom = r.0 * s.1 - r.1 * s.0;
    if denom.abs() < EPS {
        return None;
    }
    let q = (c.0 - a.0, c.1 - a.1);
    let t = (q.0 * s.1 - q.1 * s.0) / denom;
    let u = (q.0 * r.1 - q.1 * r.0) / denom;
    if !(-EPS..=1.0 + EPS).contains(&t) || !(-EPS..=1.0 + EPS).contains(&u) {
        return None;
    }
    Some((a.0 + t * r.0, a.1 + t * r.1))
}

#[cfg(test)]
mod tests;
