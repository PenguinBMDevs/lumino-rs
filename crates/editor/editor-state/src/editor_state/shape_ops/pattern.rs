//! Formula / Pattern 几何（DRAW-016）
//!
//! 移植自 Spiderweb `notes/pattern.py` 的核心口径：
//! - 曲线按屏幕弧长参数化（`x = tick/(ppq·k)`、`y = key`）；
//! - 每圈 `LOOP_SAMPLES` 个点，`loops` 圈沿曲线铺开；
//! - 侧向 = 前进方向的**左法线** × 公式值 × scale；
//! - `along` 把点沿曲线前后平移（负值回退形成卷曲，是特性）；
//! - `mirror` 取另一侧，`sym` 后半段镜像/转半圈；
//! - 超过 `MAX_POINTS` 截断，不 panic / 不 OOM。

use lumino_core::shape::expr::compile;
use lumino_core::shape::{LoopCurve, PatternParams, Sym};

use crate::shape_ops::arc::arc_k_or_one;

/// 路径点（tick, key）
pub type Pt = (f64, f64);

/// 每圈采样数（4 的倍数，锯齿尖端精确）
pub const LOOP_SAMPLES: usize = 256;
/// 输出点数上限
pub const MAX_POINTS: usize = 400_000;

const EPS: f64 = 1e-12;

/// 应用 Pattern：失败/退化时原样返回路径（不 panic）
pub fn formed_path(path: &[Pt], params: &PatternParams, ppq: f64) -> Vec<Pt> {
    if path.len() < 2 || !ppq.is_finite() || ppq <= 0.0 {
        return path.to_vec();
    }
    let k = arc_k_or_one(f64::from(params.k));
    let scale = ppq * k;
    let screen = path
        .iter()
        .map(|(tick, key)| (tick / scale, *key))
        .collect::<Vec<_>>();
    let (lengths, total) = lengths_of(&screen);
    if total <= EPS {
        return path.to_vec();
    }
    let Some((u, v)) = loop_values(params) else {
        return path.to_vec();
    };
    if u.len() < 2 || u.len() != v.len() {
        return path.to_vec();
    }
    let loops = f64::from(params.loops.max(1e-6));
    let loop_count = loops.ceil().max(1.0) as usize;
    let mut entries: Vec<(f64, f64)> = Vec::new();
    for loop_index in 0..loop_count {
        for i in 0..u.len().saturating_sub(1) {
            let along = u[i] + loop_index as f64;
            if along <= loops + 1e-9 {
                entries.push((along, v[i]));
                if entries.len() >= MAX_POINTS {
                    break;
                }
            }
        }
        if entries.len() >= MAX_POINTS {
            break;
        }
    }
    let last_side = interpolate_unit(&u, &v, loops.fract());
    entries.push((loops, last_side));
    if entries.len() < 2 {
        return path.to_vec();
    }
    let normals = normals_of(&screen);
    let mut out = Vec::with_capacity(entries.len());
    for (along, side) in entries {
        let s = along / loops * total;
        let (px, py, nx, ny) = lay(&screen, &lengths, &normals, s);
        out.push(((px + nx * side) * scale, py + ny * side));
        if out.len() >= MAX_POINTS {
            break;
        }
    }
    if out.is_empty() { path.to_vec() } else { out }
}

fn loop_values(params: &PatternParams) -> Option<(Vec<f64>, Vec<f64>)> {
    if let Some(loop_curve) = &params.loop_curve {
        return loop_curve_values(loop_curve, params);
    }
    let vars = params.vars.keys().map(String::as_str).collect::<Vec<_>>();
    let formula = compile(&params.formula, &vars).ok()?;
    let along = params
        .along
        .as_ref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| compile(text, &vars))
        .transpose()
        .ok()?;
    let half = params.sym.is_some();
    let count = if half { LOOP_SAMPLES / 2 } else { LOOP_SAMPLES };
    let mut us = Vec::with_capacity(count + 1);
    let mut vs = Vec::with_capacity(count + 1);
    let mut shifts = Vec::with_capacity(count + 1);
    for i in 0..=count {
        let x = i as f64 / count as f64;
        let y = formula.eval(x, &params.vars).ok()?;
        if !y.is_finite() {
            return None;
        }
        let shift = match &along {
            Some(expr) => {
                let value = expr.eval(x, &params.vars).ok()?;
                if !value.is_finite() {
                    return None;
                }
                value
            }
            None => 0.0,
        };
        us.push(x);
        vs.push(y);
        shifts.push(shift);
    }
    if let Some(sym) = params.sym {
        let mut mirrored = Vec::with_capacity(us.len() * 2 - 1);
        let mut mirrored_u = Vec::with_capacity(us.len() * 2 - 1);
        let mut mirrored_a = Vec::with_capacity(us.len() * 2 - 1);
        mirrored.extend_from_slice(&vs);
        mirrored_u.extend_from_slice(&us);
        mirrored_a.extend_from_slice(&shifts);
        match sym {
            Sym::Mirror => {
                for i in (0..vs.len() - 1).rev() {
                    mirrored.push(vs[i]);
                    mirrored_u.push(us[i]);
                    mirrored_a.push(-shifts[i]);
                }
            }
            Sym::Turn => {
                let last = *vs.last()?;
                for i in (0..vs.len() - 1).rev() {
                    mirrored.push(2.0 * last - vs[i]);
                    mirrored_u.push(us[i]);
                    mirrored_a.push(-shifts[i]);
                }
            }
        }
        us = mirrored_u;
        vs = mirrored;
        shifts = mirrored_a;
    }
    let factor = if params.mirror {
        -f64::from(params.scale)
    } else {
        f64::from(params.scale)
    };
    for (u, shift) in us.iter_mut().zip(shifts) {
        *u += shift;
    }
    for value in &mut vs {
        *value *= factor;
    }
    Some((us, vs))
}

fn loop_curve_values(curve: &LoopCurve, params: &PatternParams) -> Option<(Vec<f64>, Vec<f64>)> {
    if curve.pts.len() < 2 {
        return None;
    }
    let factor = if params.mirror {
        -f64::from(params.scale)
    } else {
        f64::from(params.scale)
    };
    let mut us = Vec::with_capacity(curve.pts.len());
    let mut vs = Vec::with_capacity(curve.pts.len());
    for point in &curve.pts {
        if !point[0].is_finite() || !point[1].is_finite() {
            return None;
        }
        us.push(f64::from(point[0]));
        vs.push(f64::from(point[1]) * factor);
    }
    Some((us, vs))
}

fn lengths_of(points: &[Pt]) -> (Vec<f64>, f64) {
    let mut lengths = Vec::with_capacity(points.len());
    lengths.push(0.0);
    for pair in points.windows(2) {
        let d = ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt();
        let last = lengths.last().copied().unwrap_or(0.0);
        lengths.push(last + d);
    }
    let total = lengths.last().copied().unwrap_or(0.0);
    (lengths, total)
}

fn normals_of(points: &[Pt]) -> Vec<Pt> {
    let n = points.len();
    if n < 2 {
        return vec![(0.0, 1.0)];
    }
    let mut dirs = Vec::with_capacity(n - 1);
    for pair in points.windows(2) {
        let (dx, dy) = (pair[1].0 - pair[0].0, pair[1].1 - pair[0].1);
        let len = (dx * dx + dy * dy).sqrt();
        dirs.push(if len > EPS {
            (dx / len, dy / len)
        } else {
            (1.0, 0.0)
        });
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let tangent = if i == 0 {
            dirs[0]
        } else if i == n - 1 {
            dirs[n - 2]
        } else {
            let (ax, ay) = dirs[i - 1];
            let (bx, by) = dirs[i];
            let (x, y) = (ax + bx, ay + by);
            let len = (x * x + y * y).sqrt();
            if len > EPS {
                (x / len, y / len)
            } else {
                (bx, by)
            }
        };
        out.push((-tangent.1, tangent.0));
    }
    out
}

/// 在曲线屏幕坐标上取 `s` 处的位置与左法线；越界沿端点切线延伸
fn lay(points: &[Pt], lengths: &[f64], normals: &[Pt], s: f64) -> (f64, f64, f64, f64) {
    let total = lengths.last().copied().unwrap_or(0.0);
    if s <= 0.0 {
        let (nx, ny) = normals[0];
        let (tx, ty) = (ny, -nx);
        return (points[0].0 + tx * s, points[0].1 + ty * s, nx, ny);
    }
    if s >= total {
        let end = points.len() - 1;
        let (nx, ny) = normals[end];
        let (tx, ty) = (ny, -nx);
        let over = s - total;
        return (points[end].0 + tx * over, points[end].1 + ty * over, nx, ny);
    }
    let index = lengths
        .windows(2)
        .position(|pair| s >= pair[0] && s <= pair[1])
        .unwrap_or(0);
    let span = lengths[index + 1] - lengths[index];
    let t = if span > EPS {
        (s - lengths[index]) / span
    } else {
        0.0
    };
    let px = points[index].0 + (points[index + 1].0 - points[index].0) * t;
    let py = points[index].1 + (points[index + 1].1 - points[index].1) * t;
    let nx = normals[index].0 + (normals[index + 1].0 - normals[index].0) * t;
    let ny = normals[index].1 + (normals[index + 1].1 - normals[index].1) * t;
    let len = (nx * nx + ny * ny).sqrt();
    if len > EPS {
        (px, py, nx / len, ny / len)
    } else {
        (px, py, normals[index].0, normals[index].1)
    }
}

fn interpolate_unit(u: &[f64], v: &[f64], x: f64) -> f64 {
    if u.len() < 2 || u.len() != v.len() {
        return 0.0;
    }
    if x <= u[0] {
        return v[0];
    }
    for i in 0..u.len() - 1 {
        if x <= u[i + 1] {
            let span = u[i + 1] - u[i];
            let t = if span.abs() > EPS {
                (x - u[i]) / span
            } else {
                0.0
            };
            return v[i] + (v[i + 1] - v[i]) * t;
        }
    }
    *v.last().unwrap_or(&0.0)
}

/// 预设变量表
pub type PresetVars = &'static [(&'static str, f64)];

#[cfg(test)]
mod tests;
