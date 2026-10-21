//! 形状 → 贝塞尔锚点（手柄收口）
//!
//! 统一复用核心 [`ShapeAnchor`]：线/折线/手绘/曲线/弧以及框型几何都转换成
//! 同一种锚点结构，控制柄不丢失。曲线工具、Join、后续 Symmetric / Between
//! 共用本模块，禁止再各写一套转换。

use lumino_core::shape::{ShapeAnchor, ShapeKind, ShapeModel, ShapeRect};

/// 形状 → 贝塞尔锚点；`sharp` 为尖角锚点序号
///
/// - 有显式锚点时原样保留（含 `in_handle` / `out_handle` / `handles_auto`）
/// - 只有框型几何（Custom / Funnel）时，按四角展开为闭合锚点链
/// - 弧保留三点，不做升阶/采样
pub fn to_bezier(shape: &ShapeModel, _k: f32) -> (Vec<ShapeAnchor>, Vec<u16>) {
    if !shape.geometry.anchors.is_empty() {
        let sharp = match shape.kind {
            ShapeKind::Polyline | ShapeKind::Freehand => {
                (1..shape.geometry.anchors.len().saturating_sub(1))
                    .map(|index| index as u16)
                    .collect()
            }
            _ => Vec::new(),
        };
        return (shape.geometry.anchors.clone(), sharp);
    }
    if let Some(rect) = shape.geometry.rect {
        return (rect_anchors(rect), vec![1, 2, 3]);
    }
    (Vec::new(), Vec::new())
}

/// 反向锚点链：顺序反转，同时交换每个锚点的入/出控制柄
pub fn reverse_anchors(anchors: &mut [ShapeAnchor]) {
    anchors.reverse();
    for anchor in anchors {
        anchor.swap_handles();
    }
}

fn rect_anchors(rect: ShapeRect) -> Vec<ShapeAnchor> {
    [
        (rect.tick_lo, rect.key_lo),
        (rect.tick_hi, rect.key_lo),
        (rect.tick_hi, rect.key_hi),
        (rect.tick_lo, rect.key_hi),
    ]
    .into_iter()
    .map(ShapeAnchor::new)
    .collect()
}

#[cfg(test)]
mod tests;
