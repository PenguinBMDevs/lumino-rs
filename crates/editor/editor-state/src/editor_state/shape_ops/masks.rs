//! Mask 区域代数与组合（DRAW-010 第一段）
//!
//! 纯函数层：不碰渲染、不改底层音符。区域以逐 key 行的 tick 区间表示，
//! group 按 Shapes 列表顺序组合；`parts` 提供显示/Apply 用的可见片段。

pub mod area;
pub mod group;
pub mod parts;

use lumino_core::shape::{MaskParams, MaskShow, ShapeFill, ShapeModel, ShapeOp};

use crate::ShapeNote;

pub use area::{Area, MaskAreaInput, RowSpan, band_area, base_areas, inside_area};
pub use group::{MaskMember, UnitArea, unit_area};
pub use parts::{MaskPreview, MaskedPiece, apply_visible, covered, preview, shown_parts};

/// 形状上的 Mask 参数（没有则 `None`）
pub fn params_of(model: &ShapeModel) -> Option<&MaskParams> {
    model.ops.iter().find_map(|op| match op {
        ShapeOp::Mask(params) => Some(params),
        _ => None,
    })
}

/// Fill / Spam 形状：遮罩取整个内部（上游 `SOLID`）
pub fn is_solid(model: &ShapeModel) -> bool {
    matches!(model.style.fill, ShapeFill::Fill | ShapeFill::Spam)
}

/// 遮罩的闭合轮廓（优先锚点；纯 rect 几何用四角兜底）
pub fn polygon_of(model: &ShapeModel) -> Option<Vec<(f32, f32)>> {
    if model.geometry.anchors.len() >= 3 && model.geometry.closed {
        return Some(
            model
                .geometry
                .anchors
                .iter()
                .map(|anchor| anchor.pos)
                .collect(),
        );
    }
    let rect = model.geometry.rect?;
    if !model.geometry.closed {
        return None;
    }
    Some(vec![
        (rect.tick_lo, rect.key_lo),
        (rect.tick_hi, rect.key_lo),
        (rect.tick_hi, rect.key_hi),
        (rect.tick_lo, rect.key_hi),
    ])
}

/// 从一个形状 + 它的音符构造 group 成员
pub fn member_of(
    model: &ShapeModel,
    notes: &[ShapeNote],
    ppq: f64,
    nkeys: u16,
) -> Option<MaskMember> {
    let params = params_of(model)?.clone();
    let polygon = polygon_of(model);
    let (base, own) = base_areas(&MaskAreaInput {
        notes,
        polygon: polygon.as_deref(),
        solid: is_solid(model),
        show_line: params.show == MaskShow::Line,
        own: params.own,
        size_keys: params.size_keys,
        tpk: params.k * ppq as f32,
        nkeys,
    });
    Some(MaskMember { params, base, own })
}

#[cfg(test)]
mod tests;
