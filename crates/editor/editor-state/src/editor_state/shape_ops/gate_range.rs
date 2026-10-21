//! Gate Range 纯算法（DRAW-013）
//!
//! 移植自 Spiderweb `gaterange.py`：`gate_at` / `steps_of` / `range_squares` /
//! `RangeKeys`。门长全部按整 tick；graph 线性插值。

use lumino_core::shape::{GateRangeParams, RangeDir};

const EPS: f64 = 1e-9;

/// graph 插值 + 整 tick 门长
pub fn gate_at(graph: &[[f32; 2]], a: i64, b: i64, u: f32, halves: bool) -> i64 {
    let y = interp(graph, u.clamp(0.0, 1.0));
    y_gate(a, b, y, halves)
}

fn y_gate(a: i64, b: i64, y: f32, halves: bool) -> i64 {
    let n = (b - a).abs() + 1;
    let step = ((y as f64 * n as f64).floor() as i64).min(n - 1);
    let gate = a + if b >= a { step } else { -step };
    if halves { halved(gate, a, b) } else { gate }
}

fn halved(g: i64, a: i64, b: i64) -> i64 {
    if a == b || a <= 0 || b <= 0 {
        return g;
    }
    let steps = steps_to(a, b);
    let k = (f64::from(g as i32) / f64::from(a as i32)).log2().round() as i32;
    let lo = steps.min(0);
    let hi = steps.max(0);
    let k = k.clamp(lo, hi);
    (a as f64 * 2f64.powi(k)).round() as i64
}

fn steps_to(a: i64, b: i64) -> i32 {
    if a <= 0 || b <= 0 || a == b {
        return 0;
    }
    let x = (b as f64 / a as f64).log2();
    if x > 0.0 {
        (x + 1e-9).floor() as i32
    } else {
        (x - 1e-9).ceil() as i32
    }
}

/// `[(u0, u1, gate)]`：graph 上每个门长保持的区间（相邻同门合并）
pub fn steps_of(graph: &[[f32; 2]], a: i64, b: i64, halves: bool) -> Vec<(f32, f32, i64)> {
    if graph.len() < 2 {
        return vec![(0.0, 1.0, y_gate(a, b, 0.0, halves))];
    }
    let n = (b - a).abs() + 1;
    let mut cuts = vec![0.0_f32, 1.0];
    for pair in graph.windows(2) {
        let ([u0, y0], [u1, y1]) = (pair[0], pair[1]);
        cuts.push(u0);
        if y1 != y0 && u1 > u0 {
            let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
            let k0 = (f64::from(lo) * n as f64).ceil() as i64;
            let k1 = (f64::from(hi) * n as f64).floor() as i64;
            for k in k0..=k1 {
                let u = f64::from(u0)
                    + (k as f64 / n as f64 - f64::from(y0)) * f64::from(u1 - u0)
                        / f64::from(y1 - y0);
                let u = u as f32;
                if u > u0 && u < u1 {
                    cuts.push(u);
                }
            }
        }
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup_by(|a, b| (*a - *b).abs() <= 1e-6);

    let mut out: Vec<(f32, f32, i64)> = Vec::new();
    for pair in cuts.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        if f64::from(q - p) <= EPS {
            continue;
        }
        let y = interp(graph, (p + q) * 0.5);
        let gate = y_gate(a, b, y, halves);
        match out.last_mut() {
            Some(last) if last.2 == gate && (last.1 - p).abs() <= 1e-6 => last.1 = q,
            _ => out.push((p, q, gate)),
        }
    }
    out
}

/// 从 `t0` 到 `t1`（可越过）背靠背铺门，返回 `(start, end)` 片段
pub fn range_squares(
    t0: i64,
    t1: i64,
    a: i64,
    b: i64,
    graph: &[[f32; 2]],
    halves: bool,
) -> Vec<(i64, i64)> {
    let span = (t1 - t0).max(1) as f64;
    let steps = steps_of(graph, a, b, halves);
    if steps.is_empty() {
        return Vec::new();
    }
    let ends: Vec<f64> = steps
        .iter()
        .map(|(_, u1, _)| t0 as f64 + f64::from(*u1) * span)
        .collect();
    let gates: Vec<i64> = steps.iter().map(|(_, _, g)| *g).collect();
    let mut t = t0;
    let mut index = 0;
    let mut parts: Vec<i64> = Vec::new();
    loop {
        while index < ends.len() && ends[index] <= t as f64 {
            index += 1;
        }
        if index >= ends.len() {
            break;
        }
        let gate = gates[index];
        let n = ((ends[index] - t as f64) / gate as f64).ceil().max(1.0) as i64;
        for k in 0..=n {
            parts.push(t + gate * k);
        }
        t += gate * n;
    }
    let gate = *gates.last().unwrap_or(&a);
    parts.push(t + gate);
    parts.push(t + gate * 2);
    parts.sort_unstable();
    parts.dedup();
    parts
        .windows(2)
        .filter(|pair| pair[1] > pair[0])
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

/// 每个 key 行恒定门长（`dir=keys`）
pub fn keys_gate(
    key: u16,
    k0: u16,
    k1: u16,
    a: i64,
    b: i64,
    graph: &[[f32; 2]],
    halves: bool,
) -> i64 {
    let u = if k1 > k0 {
        f32::from(key.saturating_sub(k0)) / f32::from(k1 - k0)
    } else {
        0.0
    };
    gate_at(graph, a, b, u.clamp(0.0, 1.0), halves)
}

/// `keys_down` 等价于 `keys` + 反向 graph
pub fn normalize(params: &GateRangeParams) -> (RangeDir, Vec<[f32; 2]>) {
    if params.dir == RangeDir::KeysDown {
        let graph = params
            .graph
            .iter()
            .rev()
            .map(|[u, y]| [1.0 - u, *y])
            .collect();
        (RangeDir::Keys, graph)
    } else {
        (params.dir, params.graph.clone())
    }
}

/// `fit`：首片贴 `t0`、末片贴 `t1`
pub fn fit_squares(squares: &mut [(i64, i64)], t0: i64, t1: i64) {
    if squares.is_empty() {
        return;
    }
    squares[0].0 = t0;
    let last = squares.len() - 1;
    squares[last].1 = t1;
}

fn interp(graph: &[[f32; 2]], u: f32) -> f32 {
    if graph.is_empty() {
        return 0.0;
    }
    if u <= graph[0][0] {
        return graph[0][1];
    }
    for pair in graph.windows(2) {
        let ([u0, y0], [u1, y1]) = (pair[0], pair[1]);
        if u <= u1 {
            if (u1 - u0).abs() <= f32::EPSILON {
                return y1;
            }
            return y0 + (y1 - y0) * (u - u0) / (u1 - u0);
        }
    }
    graph[graph.len() - 1][1]
}

#[cfg(test)]
mod tests;
