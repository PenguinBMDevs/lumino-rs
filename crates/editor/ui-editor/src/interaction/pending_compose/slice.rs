//! DRAW-041C / DRAW-025：Slice 组合的几何辅助（任意方向切割）。
//!
//! 从 `pending_compose.rs` 拆出（文件长度纪律）。切线由调用方先对形状外接框
//! 做延长（`extend_line`），保证线段覆盖整个形状。
//!
//! 两半的顺序必须与 `split_notes_line` 一致（小 tick 侧在前），否则几何会
//! 和另一半的音符配错；`cut_loops` 只按环序返回两半，因此这里用质心判侧重排。
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel};
use lumino_editor_state::shape_ops::{Pt, cut_loops, cut_open_line, normalize_line, ring_of};

/// 两/多段闭环拍平成一条折线模型（Slice 的两片）
pub(super) fn polyline_model(
    points: Vec<Vec<(f32, f32)>>,
    track: usize,
    k: f32,
    closed: bool,
) -> ShapeModel {
    let points: Vec<(f32, f32)> = points.into_iter().flatten().collect();
    let mut model = ShapeModel::new(0, track as u16, ShapeKind::Polyline, k);
    model.geometry = ShapeGeometry::from_points(points);
    model.geometry.closed = closed;
    model
}

/// 切出的两半几何（与外层 Vec 分支同形）
type SliceHalves = (Vec<Vec<(f32, f32)>>, Vec<Vec<(f32, f32)>>);

/// 闭环优先走 `cut_loops`；开放折线（曲线/弧的展平）走直线切段。
pub(super) fn split_slice_parts(model: &ShapeModel, a: Pt, b: Pt) -> Option<SliceHalves> {
    let (a, b) = normalize_line(a, b);
    if model.geometry.closed
        && let Some((first, second)) = cut_loops(&[ring_of(model)], a, b)
    {
        return Some(order_halves(first, second, a, b));
    }
    split_open_model(model, a, b)
}

/// 按质心所在侧重排两半：与 `split_notes_line` 的「小 tick 侧在前」对齐
fn order_halves(first: SliceHalvesPart, second: SliceHalvesPart, a: Pt, b: Pt) -> SliceHalves {
    if side_of(&first, a, b) >= side_of(&second, a, b) {
        (first, second)
    } else {
        (second, first)
    }
}

/// 一组闭环（含多环）的质心侧别：`cross(b - a, centroid - a)`
fn side_of(loops: &SliceHalvesPart, a: Pt, b: Pt) -> f64 {
    let Some(ring) = loops.first() else {
        return 0.0;
    };
    let n = ring.len().max(1) as f32;
    let cx = ring.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = ring.iter().map(|p| p.1).sum::<f32>() / n;
    f64::from(b.0 - a.0) * f64::from(cy - a.1) - f64::from(b.1 - a.1) * f64::from(cx - a.0)
}

/// 单侧闭环集合
type SliceHalvesPart = Vec<Vec<(f32, f32)>>;

/// 开放折线的直线切回退（`cut_loops` 只处理闭环）。
fn split_open_model(model: &ShapeModel, a: Pt, b: Pt) -> Option<SliceHalves> {
    let points = if model.geometry.closed || model.geometry.anchors.is_empty() {
        ring_of(model)
    } else {
        model
            .geometry
            .anchors
            .iter()
            .map(|anchor| anchor.pos)
            .collect()
    };
    cut_open_line(&points, a, b).map(|(first, second)| (vec![first], vec![second]))
}
