//! DRAW-041C：Slice 组合的几何辅助（竖切两半 / 开放折线回退）。
//!
//! 从 `pending_compose.rs` 拆出（文件长度纪律）。
use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel};
use lumino_editor_state::shape_ops::{cut_loops, cut_open_vertical, ring_of};

/// 两/多段闭环拍平成一条折线模型（Slice 的左右两片）
pub(super) fn polyline_model(points: Vec<Vec<(f32, f32)>>, track: usize, k: f32) -> ShapeModel {
    let points: Vec<(f32, f32)> = points.into_iter().flatten().collect();
    let mut model = ShapeModel::new(0, track as u16, ShapeKind::Polyline, k);
    model.geometry = ShapeGeometry::from_points(points);
    model
}

/// 竖切两半的几何集合（与外层 Vec 分支同形）
type SliceHalves = (Vec<Vec<(f32, f32)>>, Vec<Vec<(f32, f32)>>);

/// 闭环优先走 `cut_loops`；开放折线（曲线/弧的展平）走竖线切段。
pub(super) fn split_slice_parts(
    model: &ShapeModel,
    cut: f32,
    y0: f32,
    y1: f32,
) -> Option<SliceHalves> {
    if model.geometry.closed {
        cut_loops(&[ring_of(model)], (cut, y0 - 1.0), (cut, y1 + 1.0))
            .or_else(|| split_open_model(model, cut))
    } else {
        split_open_model(model, cut)
    }
}

/// 开放折线的竖切回退（`cut_loops` 只处理闭环）。
fn split_open_model(model: &ShapeModel, cut: f32) -> Option<SliceHalves> {
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
    cut_open_vertical(&points, cut).map(|(left, right)| (vec![left], vec![right]))
}
