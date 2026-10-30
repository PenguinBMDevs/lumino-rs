//! Bake 前组合会话（DRAW-041C）
//!
//! 组合只发生在 ✓ 前：pending 矢量 → 组合结果仍是 pending（预览 + 可撤销），
//! ✓ 才写文档音符并保留只读模型。本模块只存数据，不碰文档历史。

use lumino_core::shape::{ShapeModel, ShapeOp};

use super::line_tool::LinePath;
use super::shape_tool::ShapeInstance;
use crate::ShapeNote;

/// 参与组合的待确认来源
#[derive(Debug, Clone, PartialEq)]
pub enum PendingSource {
    /// 形状工具待确认图形
    Shape(ShapeInstance),
    /// 曲线工具待确认路径
    CurvePath(LinePath),
}

/// 组合类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposedKind {
    /// 019：多形状折叠为 Live shape
    Live,
    /// 021：多形状连接为一条曲线
    Join,
    /// 022：两形状门控合并
    Merge,
}

/// 一次 Bake 前组合的结果；`sources` 用于撤销回 pending 矢量
#[derive(Debug, Clone, PartialEq)]
pub struct PendingComposed {
    /// 组合类型
    pub kind: ComposedKind,
    /// 组合后的模型（几何/算子齐备）
    pub model: ShapeModel,
    /// 预览音符；✓ 时写入文档
    pub notes: Vec<ShapeNote>,
    /// 参与组合的 pending 形状/路径（按组合前顺序）
    pub sources: Vec<PendingSource>,
}

impl PendingComposed {
    /// 整体平移组合结果（预览 + 模型一起动；sources 已从工具容器摘除，无需回写）
    pub fn translate(&mut self, dtick: f32, dkey: f32) {
        translate_model(&mut self.model, dtick, dkey, 0);
        for note in &mut self.notes {
            *note = note.translated(dtick, dkey);
        }
    }
}

/// 平移模型几何（含 Live 的内嵌 strokes / from，深度上限防递归）
fn translate_model(model: &mut ShapeModel, dtick: f32, dkey: f32, depth: usize) {
    if depth > 8 {
        return;
    }
    for anchor in &mut model.geometry.anchors {
        anchor.pos = (
            (anchor.pos.0 + dtick).max(0.0),
            (anchor.pos.1 + dkey).clamp(0.0, 255.0),
        );
    }
    if let Some(rect) = &mut model.geometry.rect {
        rect.tick_lo = (rect.tick_lo + dtick).max(0.0);
        rect.tick_hi = (rect.tick_hi + dtick).max(0.0);
        rect.key_lo = (rect.key_lo + dkey).clamp(0.0, 255.0);
        rect.key_hi = (rect.key_hi + dkey).clamp(0.0, 255.0);
    }
    for op in &mut model.ops {
        if let ShapeOp::Live(params) = op {
            for stroke in &mut params.strokes {
                for anchor in &mut stroke.anchors {
                    anchor.pos = (
                        (anchor.pos.0 + dtick).max(0.0),
                        (anchor.pos.1 + dkey).clamp(0.0, 255.0),
                    );
                }
            }
            for from in &mut params.from {
                translate_model(from, dtick, dkey, depth + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composed_kind_is_copy() {
        let kind = ComposedKind::Live;
        let copy = kind;
        assert_eq!(copy, ComposedKind::Live);
    }
}
