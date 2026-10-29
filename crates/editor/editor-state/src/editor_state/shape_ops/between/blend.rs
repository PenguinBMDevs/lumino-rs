//! Between 形状插值

use lumino_core::shape::{AnchorBend, ShapeAnchor, ShapeGeometry, ShapeKind, ShapeModel};

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
    model
}

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

fn lerp(a: (f32, f32), b: (f32, f32), f: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
}

#[cfg(test)]
mod tests;
