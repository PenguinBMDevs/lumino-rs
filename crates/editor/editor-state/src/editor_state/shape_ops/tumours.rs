//! Tumours 几何算法：沿路径布置凸起，产出新路径点列
//!
//! 语义照 Spiderweb `notes/tumour.py` 的**核心口径**（DRAW-003）：
//! - 路径按弧长参数化（度量含 `k`：1 key = `k` 拍）；
//! - 沿 `start..end` 按 `dist` 布凸起起点；
//! - 四种形态（triangle / square / circle / parabola），`size`(key) × `length`(拍)；
//! - `side` 四值、`rot` 绕两脚倾斜、`slant` 仅 square、`ease` 两端渐入；
//! - 与下一凸起重叠时按下一凸起起点截断（cut，不叠加）；
//! - `fit` 微调距离使整数个凸起铺满；`graphs` 按 0..1 位置做线性倍率。
//!
//! 本卡先落「路径 → 路径」的纯几何；音符生成仍走 ui-editor 的
//! `flatten_path → outline_notes → keep_longest` 同源链路。

use lumino_core::shape::{TumourParams, TumourShape, TumourSide, factor_at};

/// 路径点（tick, key）
pub type Pt = (f64, f64);

/// 凸起上限（上游 `MAX_TUMOURS`）
pub const MAX_TUMOURS: usize = 20_000;

const CURVE_STEPS: usize = 8;
const EPS: f64 = 1e-9;

/// 应用结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TumourReport {
    /// 实际生成的凸起数
    pub bumps: usize,
    /// 是否因上限被截断
    pub truncated: bool,
}

/// 应用 Tumours：`ppq` 用于把参数里的「拍」换算成路径的 tick
pub fn apply(path: &[Pt], params: &TumourParams, ppq: f64) -> (Vec<Pt>, TumourReport) {
    if !params.on || path.len() < 2 || params.size == 0.0 {
        return (path.to_vec(), TumourReport::default());
    }
    let ppq = if ppq.is_finite() && ppq > 0.0 {
        ppq
    } else {
        return (path.to_vec(), TumourReport::default());
    };
    let tick_per_key = params.k as f64 * ppq;
    if !tick_per_key.is_finite() || tick_per_key <= 0.0 {
        return (path.to_vec(), TumourReport::default());
    }

    let sampler = Sampler::new(path, tick_per_key);
    if sampler.total <= EPS {
        return (path.to_vec(), TumourReport::default());
    }

    let (start, end) = if params.start <= params.end {
        (params.start as f64, params.end as f64)
    } else {
        (params.end as f64, params.start as f64)
    };
    let range_start = start * sampler.total;
    let range_end = end * sampler.total;
    let range = range_end - range_start;
    if range <= EPS {
        return (path.to_vec(), TumourReport::default());
    }

    let base_dist = params.dist as f64 * ppq;
    let mut dist = base_dist;
    let mut fit_count = None;
    if params.fit {
        let count = ((range / dist).round() as usize).max(1);
        dist = range / count as f64;
        fit_count = Some(count);
    }
    let wanted = fit_count
        .unwrap_or_else(|| ((range / dist).ceil() as usize).max(1))
        .max(1);
    let limit = wanted.min(MAX_TUMOURS);

    let mut bumps: Vec<Bump> = Vec::with_capacity(limit.min(256));
    let mut s = range_start;
    let ease_ticks = params.ease as f64 * ppq;

    while s < range_end - EPS && bumps.len() < limit {
        let u = (s / sampler.total).clamp(0.0, 1.0) as f32;
        let size_factor = factor_at(&params.graphs.size, u) as f64;
        let length_factor = (factor_at(&params.graphs.length, u) as f64).max(0.0);
        // graphs.dist 当前按「逐段倍率」近似；上游的 1/距离积分（graph_starts）留二期。
        let dist_factor = (factor_at(&params.graphs.dist, u) as f64).max(0.01);
        let rot_factor = factor_at(&params.graphs.rot, u) as f64;
        let slant_factor = factor_at(&params.graphs.slant, u) as f64;

        let next = (s + (dist * dist_factor).max(EPS)).min(range_end);
        let length = (params.length as f64 * length_factor * ppq).max(0.0);
        // 重叠截断：当前凸起在下一个起点处结束（cut，不叠加）
        let bump_end = (s + length).min(range_end).min(next);

        let mut size = params.size as f64 * size_factor;
        if ease_ticks > EPS {
            let d_in = s - range_start;
            let d_out = range_end - bump_end;
            let e = (d_in.min(d_out) / ease_ticks).clamp(0.0, 1.0);
            size *= e * e * (3.0 - 2.0 * e);
        }

        bumps.push(Bump {
            s,
            end: bump_end,
            size,
            rot: params.rot as f64 * rot_factor,
            slant: (params.slant as f64 * slant_factor).clamp(-1.0, 1.0),
        });

        if next <= s {
            break;
        }
        s = next;
    }

    let truncated = wanted > MAX_TUMOURS || (s < range_end - EPS && bumps.len() >= limit);
    let report = TumourReport {
        bumps: bumps.len(),
        truncated,
    };

    let mut out = Vec::with_capacity(path.len() + bumps.len() * 12);
    push_distinct(&mut out, path[0]);
    let mut cursor = 0.0;
    for (index, bump) in bumps.iter().enumerate() {
        append_vertices(&mut out, path, &sampler.cum, cursor, bump.s);
        for point in bump_points(&sampler, bump, params, index) {
            push_distinct(&mut out, point);
        }
        cursor = bump.end;
    }
    append_vertices(&mut out, path, &sampler.cum, cursor, sampler.total);
    push_distinct(&mut out, path[path.len() - 1]);

    (out, report)
}

struct Bump {
    s: f64,
    end: f64,
    size: f64,
    rot: f64,
    slant: f64,
}

struct Sampler<'a> {
    pts: &'a [Pt],
    cum: Vec<f64>,
    total: f64,
    tick_per_key: f64,
}

impl<'a> Sampler<'a> {
    fn new(pts: &'a [Pt], tick_per_key: f64) -> Self {
        let mut cum = Vec::with_capacity(pts.len());
        cum.push(0.0);
        let mut total = 0.0;
        for pair in pts.windows(2) {
            let dt = pair[1].0 - pair[0].0;
            let dk = pair[1].1 - pair[0].1;
            total += (dt * dt + (dk * tick_per_key).powi(2)).sqrt();
            cum.push(total);
        }
        Self {
            pts,
            cum,
            total,
            tick_per_key,
        }
    }

    /// 弧长 `s` 处的点与单位法线（法线以 key 为单位）
    fn point_at(&self, s: f64) -> (Pt, Pt) {
        let s = s.clamp(0.0, self.total);
        let mut lo = 0;
        let mut hi = self.cum.len() - 1;
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.cum[mid] <= s {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let i = lo.min(self.pts.len() - 2);
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg <= EPS {
            0.0
        } else {
            ((s - self.cum[i]) / seg).clamp(0.0, 1.0)
        };
        let a = self.pts[i];
        let b = self.pts[i + 1];
        let point = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        (point, self.normal_of(a, b))
    }

    fn normal_of(&self, a: Pt, b: Pt) -> Pt {
        let dt = b.0 - a.0;
        let dk = b.1 - a.1;
        let len = (dt * dt + (dk * self.tick_per_key).powi(2)).sqrt();
        if len <= EPS {
            return (0.0, 1.0);
        }
        // 缩放空间法线 (-sy, sx) 转回世界：x 乘 tick_per_key
        (-dk * self.tick_per_key * self.tick_per_key / len, dt / len)
    }
}

fn bump_points(sampler: &Sampler, bump: &Bump, params: &TumourParams, index: usize) -> Vec<Pt> {
    let length = (bump.end - bump.s).max(0.0);
    let height = bump.size * side_sign(params, index);
    let shear = bump.rot.to_radians().tan();
    profile(params.shape, length, height, bump.slant)
        .into_iter()
        .map(|(u, v)| {
            let along = (bump.s + u + v * sampler.tick_per_key * shear).clamp(0.0, sampler.total);
            let (base, normal) = sampler.point_at(along);
            (base.0 + normal.0 * v, base.1 + normal.1 * v)
        })
        .collect()
}

fn side_sign(params: &TumourParams, index: usize) -> f64 {
    let sign = match params.side {
        TumourSide::Left => 1.0,
        TumourSide::Right => -1.0,
        TumourSide::Alt => {
            if index.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            }
        }
        TumourSide::Random => {
            if random_bit(params.seed, index) {
                1.0
            } else {
                -1.0
            }
        }
    };
    if params.mirror { -sign } else { sign }
}

fn random_bit(seed: u32, index: usize) -> bool {
    let mut x = seed ^ (index as u32).wrapping_mul(0x9E37_79B9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x & 1 == 0
}

/// 局部坐标 `(沿路径, 法线)` 的凸起轮廓；`length == 0` 退化为尖刺
fn profile(shape: TumourShape, length: f64, height: f64, slant: f64) -> Vec<(f64, f64)> {
    if length <= EPS {
        return vec![(0.0, 0.0), (0.0, height), (0.0, 0.0)];
    }
    match shape {
        TumourShape::Triangle => vec![(0.0, 0.0), (length * 0.5, height), (length, 0.0)],
        TumourShape::Square => {
            // slant > 0 收窄顶边；slant < 0 加宽顶边（允许顶边短暂外伸）
            let inset = slant * length * 0.5;
            vec![
                (0.0, 0.0),
                (inset, height),
                (length - inset, height),
                (length, 0.0),
            ]
        }
        TumourShape::Circle | TumourShape::Parabola => {
            let mut points = Vec::with_capacity(CURVE_STEPS + 1);
            for i in 0..=CURVE_STEPS {
                let t = i as f64 / CURVE_STEPS as f64;
                let x = 2.0 * t - 1.0;
                let factor = 1.0 - x * x;
                let v = match shape {
                    TumourShape::Circle => height * factor.max(0.0).sqrt(),
                    _ => height * factor,
                };
                points.push((length * t, v));
            }
            points
        }
    }
}

fn append_vertices(out: &mut Vec<Pt>, pts: &[Pt], cum: &[f64], from: f64, to: f64) {
    for (index, &point) in pts.iter().enumerate() {
        let distance = cum[index];
        if distance > from + EPS && distance <= to + EPS {
            push_distinct(out, point);
        }
    }
}

fn push_distinct(out: &mut Vec<Pt>, point: Pt) {
    match out.last() {
        Some(last) if (last.0 - point.0).abs() <= EPS && (last.1 - point.1).abs() <= EPS => {}
        _ => out.push(point),
    }
}
