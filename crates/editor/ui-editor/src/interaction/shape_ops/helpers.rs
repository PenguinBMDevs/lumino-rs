//! 形状再生成的小工具（从 `shape_ops.rs` 拆出，控制文件长度）

use lumino_core::shape::{ShapeModel, ShapeOp, ShapeRect};

/// `own=false` 的遮罩：自身音符只用于区域计算，不写入文档发声
pub(super) fn mask_silent(model: &ShapeModel) -> bool {
    model
        .ops
        .iter()
        .any(|op| matches!(op, ShapeOp::Mask(params) if !params.own))
}

/// 展平路径 → 形状包围盒（key 上下各扩 0.5，供 Glue 的 Boxes 使用）
pub(super) fn shape_box_of(points: &[(f64, f64)]) -> Option<ShapeRect> {
    let &(first_tick, first_key) = points.first()?;
    let mut tick_lo = first_tick;
    let mut tick_hi = first_tick;
    let mut key_lo = first_key;
    let mut key_hi = first_key;
    for &(tick, key) in points {
        tick_lo = tick_lo.min(tick);
        tick_hi = tick_hi.max(tick);
        key_lo = key_lo.min(key);
        key_hi = key_hi.max(key);
    }
    Some(ShapeRect::normalized(
        tick_lo as f32,
        key_lo as f32 - 0.5,
        tick_hi as f32,
        key_hi as f32 + 0.5,
    ))
}
