//! Between 形状插值（DRAW-026）
//!
//! 几何按锚点插值；`resample` 用 De Casteljau 子曲线补锚点，补点前后曲线外观不变。
//! `Tumours` / `Pattern` 算子随步骤收敛：双边插值，单边在中步缩到 0，再向有算子的一端长出。

use std::collections::BTreeMap;

use lumino_core::shape::{
    AnchorBend, PatternParams, ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp,
    TumourParams,
};

use super::super::bezier::{reverse_anchors, to_bezier};

/// 首/末形状按 `f` 插值；`rev` 时末形状锚点反序配对
pub fn blend(first: &ShapeModel, last: &ShapeModel, f: f32, rev: bool) -> ShapeModel {
    let f = f.clamp(0.0, 1.0);
    let (mut a, _) = to_bezier(first, first.k);
    let (mut b, _) = to_bezier(last, last.k);
    if rev {
        reverse_anchors(&mut b);
    }
    let count = a.len().max(b.len());
    if a.len() != count {
        a = resample(&a, count);
    }
    if b.len() != count {
        b = resample(&b, count);
    }
    let anchors = a
        .iter()
        .zip(b.iter())
        .map(|(a, b)| ShapeAnchor {
            pos: lerp(a.pos, b.pos, f),
            in_handle: lerp(a.in_handle, b.in_handle, f),
            out_handle: lerp(a.out_handle, b.out_handle, f),
            handles_auto: a.handles_auto && b.handles_auto,
            bend: if a.bend == b.bend {
                a.bend
            } else {
                AnchorBend::Smooth
            },
        })
        .collect::<Vec<_>>();
    let kind = if first.kind == last.kind
        && matches!(
            first.kind,
            ShapeKind::Line | ShapeKind::Polyline | ShapeKind::Curve | ShapeKind::Arc
        ) {
        first.kind
    } else {
        ShapeKind::Curve
    };
    let mut model = ShapeModel::new(0, first.track, kind, first.k);
    model.geometry = ShapeGeometry {
        anchors,
        rect: None,
        closed: false,
    };
    model.style = first.style.clone();
    model.ops = blended_ops(first, last, f);
    model
}

/// 补锚点到 `count` 个；每个原始段在自身参数域内均分，使用 De Casteljau 子曲线，
/// 因此补点前后曲线（含控制柄）保持同一外观。
fn resample(anchors: &[ShapeAnchor], count: usize) -> Vec<ShapeAnchor> {
    if count == 0 || anchors.is_empty() {
        return Vec::new();
    }
    if anchors.len() == count {
        return anchors.to_vec();
    }
    if anchors.len() == 1 {
        return vec![anchors[0]; count];
    }
    let segments = anchors.len() - 1;
    let wanted = count - 1;
    if wanted < segments {
        // 只会在“减锚点”时走到；当前 blend 只在 count 更大时调用，保留兜底。
        return linear_resample(anchors, count);
    }
    let base = wanted / segments;
    let extra = wanted % segments;
    let mut out: Vec<ShapeAnchor> = Vec::with_capacity(count);
    for index in 0..segments {
        let parts = base + usize::from(index < extra);
        let p0 = anchors[index].pos;
        let p1 = add(p0, anchors[index].out_handle);
        let p2 = add(anchors[index + 1].pos, anchors[index + 1].in_handle);
        let p3 = anchors[index + 1].pos;
        for part in 0..parts {
            let u0 = part as f32 / parts as f32;
            let u1 = (part + 1) as f32 / parts as f32;
            let [q0, q1, q2, q3] = sub_cubic(p0, p1, p2, p3, u0, u1);
            if let Some(last) = out.last_mut() {
                last.out_handle = sub(q1, q0);
            } else {
                out.push(anchor(q0, (0.0, 0.0), sub(q1, q0)));
            }
            out.push(anchor(q3, sub(q2, q3), (0.0, 0.0)));
        }
    }
    out
}

/// 兜底线性补点（当前不会被 blend 调用）
fn linear_resample(anchors: &[ShapeAnchor], count: usize) -> Vec<ShapeAnchor> {
    if count == 0 || anchors.is_empty() {
        return Vec::new();
    }
    if anchors.len() == count {
        return anchors.to_vec();
    }
    if anchors.len() == 1 {
        return vec![anchors[0]; count];
    }
    (0..count)
        .map(|index| {
            let t = index as f32 / (count - 1).max(1) as f32;
            let pos = t * (anchors.len() - 1) as f32;
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(anchors.len() - 1);
            let local = pos - lo as f32;
            let a = anchors[lo];
            let b = anchors[hi];
            ShapeAnchor {
                pos: lerp(a.pos, b.pos, local),
                in_handle: lerp(a.in_handle, b.in_handle, local),
                out_handle: lerp(a.out_handle, b.out_handle, local),
                handles_auto: a.handles_auto && b.handles_auto,
                bend: if a.bend == b.bend {
                    a.bend
                } else {
                    AnchorBend::Smooth
                },
            }
        })
        .collect()
}

fn blended_ops(first: &ShapeModel, last: &ShapeModel, f: f32) -> Vec<ShapeOp> {
    let mut ops = Vec::new();
    if let Some(params) = blend_tumours(tumour_of(first), tumour_of(last), f) {
        ops.push(ShapeOp::Tumours(params));
    }
    if let Some(params) = blend_pattern(pattern_of(first), pattern_of(last), f) {
        ops.push(ShapeOp::Pattern(params));
    }
    ops
}

fn tumour_of(model: &ShapeModel) -> Option<&TumourParams> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Tumours(params) => Some(params),
        _ => None,
    })
}

fn pattern_of(model: &ShapeModel) -> Option<&PatternParams> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Pattern(params) => Some(params),
        _ => None,
    })
}

fn blend_tumours(
    first: Option<&TumourParams>,
    last: Option<&TumourParams>,
    f: f32,
) -> Option<TumourParams> {
    match (first, last) {
        (None, None) => None,
        (Some(first), None) => Some(fade_tumour(first, 1.0 - 2.0 * f)),
        (None, Some(last)) => Some(fade_tumour(last, 2.0 * f - 1.0)),
        (Some(first), Some(last)) => Some(interpolate_tumour(first, last, f)),
    }
}

fn fade_tumour(params: &TumourParams, factor: f32) -> TumourParams {
    let mut out = params.clone();
    out.size *= factor.clamp(0.0, 1.0);
    out
}

fn interpolate_tumour(first: &TumourParams, last: &TumourParams, f: f32) -> TumourParams {
    let mut out = first.clone();
    out.size = lerp_f(first.size, last.size, f);
    out.length = lerp_f(first.length, last.length, f);
    out.dist = lerp_f(first.dist, last.dist, f);
    out.start = lerp_f(first.start, last.start, f);
    out.end = lerp_f(first.end, last.end, f);
    out.ease = lerp_f(first.ease, last.ease, f);
    out.rot = lerp_f(first.rot, last.rot, f);
    out.slant = lerp_f(first.slant, last.slant, f);
    out.k = lerp_f(first.k, last.k, f);
    if f < 0.5 {
        out.on = first.on;
        out.shape = first.shape;
        out.side = first.side;
        out.wrap = first.wrap;
        out.fit = first.fit;
        out.seed = first.seed;
        out.mirror = first.mirror;
        out.graphs = first.graphs.clone();
    } else {
        out.on = last.on;
        out.shape = last.shape;
        out.side = last.side;
        out.wrap = last.wrap;
        out.fit = last.fit;
        out.seed = last.seed;
        out.mirror = last.mirror;
        out.graphs = last.graphs.clone();
    }
    out
}

fn blend_pattern(
    first: Option<&PatternParams>,
    last: Option<&PatternParams>,
    f: f32,
) -> Option<PatternParams> {
    match (first, last) {
        (None, None) => None,
        (Some(first), None) => Some(fade_pattern(first, 1.0 - 2.0 * f)),
        (None, Some(last)) => Some(fade_pattern(last, 2.0 * f - 1.0)),
        (Some(first), Some(last)) => Some(interpolate_pattern(first, last, f)),
    }
}

fn fade_pattern(params: &PatternParams, factor: f32) -> PatternParams {
    let factor = factor.clamp(0.0, 1.0);
    let mut out = params.clone();
    out.scale *= factor;
    if let Some(height) = out.vars.get_mut("height") {
        *height *= f64::from(factor);
    }
    out
}

fn interpolate_pattern(first: &PatternParams, last: &PatternParams, f: f32) -> PatternParams {
    let mut out = if f < 0.5 { first.clone() } else { last.clone() };
    out.loops = lerp_f(first.loops, last.loops, f);
    out.k = lerp_f(first.k, last.k, f);
    out.scale = lerp_f(first.scale, last.scale, f);
    let mut vars = BTreeMap::new();
    for (key, first_value) in &first.vars {
        let value = match last.vars.get(key) {
            Some(last_value) => *first_value + (*last_value - *first_value) * f64::from(f),
            None => *first_value,
        };
        vars.insert(key.clone(), value);
    }
    for (key, last_value) in &last.vars {
        vars.entry(key.clone()).or_insert(*last_value);
    }
    out.vars = vars;
    out
}

fn anchor(pos: (f32, f32), in_handle: (f32, f32), out_handle: (f32, f32)) -> ShapeAnchor {
    let mut anchor = ShapeAnchor::new(pos);
    anchor.in_handle = in_handle;
    anchor.out_handle = out_handle;
    anchor.handles_auto = false;
    anchor.bend = AnchorBend::Smooth;
    anchor
}

/// 子曲线控制点：起点 / 起点控制 / 终点控制 / 终点
type Cubic = [(f32, f32); 4];

/// 取原三次曲线在 `[u0, u1]` 的子曲线控制点
fn sub_cubic(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    u0: f32,
    u1: f32,
) -> Cubic {
    let q0 = cubic(p0, p1, p2, p3, u0);
    let q3 = cubic(p0, p1, p2, p3, u1);
    let span = (u1 - u0) / 3.0;
    let q1 = add(q0, scale(cubic_deriv(p0, p1, p2, p3, u0), span));
    let q2 = sub(q3, scale(cubic_deriv(p0, p1, p2, p3, u1), span));
    [q0, q1, q2, q3]
}

fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

fn cubic_deriv(
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    t: f32,
) -> (f32, f32) {
    let mt = 1.0 - t;
    let d0 = 3.0 * mt * mt;
    let d1 = 6.0 * mt * t;
    let d2 = 3.0 * t * t;
    (
        d0 * (p1.0 - p0.0) + d1 * (p2.0 - p1.0) + d2 * (p3.0 - p2.0),
        d0 * (p1.1 - p0.1) + d1 * (p2.1 - p1.1) + d2 * (p3.1 - p2.1),
    )
}

fn add(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    (a.0 - b.0, a.1 - b.1)
}

fn scale(a: (f32, f32), factor: f32) -> (f32, f32) {
    (a.0 * factor, a.1 * factor)
}

fn lerp(a: (f32, f32), b: (f32, f32), f: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
}

fn lerp_f(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

#[cfg(test)]
mod tests;
