//! Turn into live shape 参数（DRAW-019）
//!
//! 把多个已有形状折叠成一个 `ShapeKind::Custom` + `ShapeOp::Live`；
//! `strokes` 保留每段类型与点集，`from` 保留原始形状供 Split separate 还原。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::{ShapeAnchor, ShapeKind, ShapeModel, ShapeOp};

/// 段类型（与 ShapeKind 对齐；Custom/Funnel 落成 Polyline）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeKind {
    /// 直线
    Line,
    /// 折线
    Polyline,
    /// 自由手绘
    Freehand,
    /// 贝塞尔曲线
    Curve,
    /// 三点圆弧
    Arc,
}

impl StrokeKind {
    /// 从形状类型映射
    pub fn from_kind(kind: ShapeKind) -> Self {
        match kind {
            ShapeKind::Line => Self::Line,
            ShapeKind::Polyline => Self::Polyline,
            ShapeKind::Freehand => Self::Freehand,
            ShapeKind::Curve => Self::Curve,
            ShapeKind::Arc => Self::Arc,
            ShapeKind::Custom | ShapeKind::Funnel => Self::Polyline,
        }
    }
}

/// 一段来源 stroke
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// 段类型
    pub kind: StrokeKind,
    /// 原始锚点（弧保留三点，曲线保留控制柄）
    pub anchors: Vec<ShapeAnchor>,
    /// 框型几何（如来源是 Custom 的 rect）
    pub rect: Option<crate::shape::ShapeRect>,
}

/// Live shape 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveShapeParams {
    /// 每段 stroke
    pub strokes: Vec<Stroke>,
    /// 每段来源形状 id
    pub src: Vec<u32>,
    /// 原始形状（还原用；不允许再含 Live）
    pub from: Vec<ShapeModel>,
}

/// 合并时丢弃设置的提示
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveWarning {
    /// 可读说明
    pub message: String,
}

impl LiveShapeParams {
    /// 校验参数；嵌套 Live 超过 2 层会被拒绝
    pub fn validate(&self) -> Result<()> {
        if self.strokes.is_empty() {
            return Err(CoreError::InvalidArgument(
                "Live 至少需要一段 stroke".into(),
            ));
        }
        if self.strokes.len() != self.src.len() || self.strokes.len() != self.from.len() {
            return Err(CoreError::InvalidArgument(
                "Live 的 strokes / src / from 长度必须一致".into(),
            ));
        }
        for (index, stroke) in self.strokes.iter().enumerate() {
            if stroke.anchors.is_empty() {
                return Err(CoreError::InvalidArgument(format!(
                    "Live 第 {index} 段没有锚点"
                )));
            }
            if stroke.anchors.iter().any(|anchor| !anchor.is_finite()) {
                return Err(CoreError::InvalidArgument(format!(
                    "Live 第 {index} 段含非有限锚点"
                )));
            }
        }
        for (index, shape) in self.from.iter().enumerate() {
            if shape.ops.iter().any(|op| matches!(op, ShapeOp::Live(_))) {
                return Err(CoreError::InvalidArgument(format!(
                    "Live 的 from[{index}] 不能再包含 Live（嵌套上限 2 层）"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
