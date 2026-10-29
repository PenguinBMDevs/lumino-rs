//! 形状库 Drawing 参数（DRAW-033A）
//!
//! 一个 Drawing 是多条 `Stroke` 的组合，落到工程里仍是一个
//! `ShapeKind::Custom` + `ShapeOp::Drawing` 的 `ShapeModel`。
//! `ShapeModel` 不新增 `strokes` 字段；Drawing 变体追加到 `ShapeOp` 末尾，
//! 因此 `.lmshapes` 仍为 v3。
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::{COLOUR_SLOT_MAX, SHAPE_KEY_MAX, ShapeAnchor, ShapeRect, Stroke};

/// 单条 stroke 的库内角色。
///
/// 首版只承载“描边色 + 是否填充”两个抽屉展示语义；音符生成仍由
/// `ShapeModel` 的 `ShapeStyle` 决定，不在本类型里另造一套生成参数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrokeRole {
    /// 颜色槽（0..=15）
    pub colour_slot: u8,
    /// 库内预览是否按闭合图形填充
    pub filled: bool,
}

impl StrokeRole {
    /// 校验颜色槽
    pub fn validate(&self) -> Result<()> {
        if self.colour_slot > COLOUR_SLOT_MAX {
            return Err(CoreError::InvalidArgument(format!(
                "StrokeRole 颜色槽越界（允许 0..={COLOUR_SLOT_MAX}，实际 {}）",
                self.colour_slot
            )));
        }
        Ok(())
    }
}

/// 一个可复用 Drawing 的工程参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingParams {
    /// 每条 stroke；坐标与 `ShapeModel` 同单位（工程内为 tick/key）
    pub strokes: Vec<Stroke>,
    /// 与 `strokes` 等长的角色；空表示全部使用默认角色
    pub roles: Vec<StrokeRole>,
}

impl DrawingParams {
    /// 单 Drawing 最大 stroke 数
    pub const MAX_STROKES: usize = 256;
    /// 单 stroke 最大锚点数
    pub const MAX_ANCHORS: usize = 4096;

    /// 校验参数；加载路径不得 panic
    pub fn validate(&self) -> Result<()> {
        if self.strokes.is_empty() {
            return Err(CoreError::InvalidArgument(
                "Drawing 至少需要一条 stroke".into(),
            ));
        }
        if self.strokes.len() > Self::MAX_STROKES {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 的 stroke 数超过上限（最多 {}，实际 {}）",
                Self::MAX_STROKES,
                self.strokes.len()
            )));
        }
        if !self.roles.is_empty() && self.roles.len() != self.strokes.len() {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 的 roles 必须为空或与 strokes 等长（strokes={}，roles={}）",
                self.strokes.len(),
                self.roles.len()
            )));
        }
        for role in &self.roles {
            role.validate()?;
        }
        for (index, stroke) in self.strokes.iter().enumerate() {
            validate_stroke(index, stroke)?;
        }
        if self.bbox().is_none() {
            return Err(CoreError::InvalidArgument(
                "Drawing 没有任何几何（stroke 锚点与外接框均为空）".into(),
            ));
        }
        Ok(())
    }

    /// 所有 stroke 的联合外接框（已规范化）；空 Drawing 返回 `None`
    pub fn bbox(&self) -> Option<ShapeRect> {
        let mut acc: Option<ShapeRect> = None;
        for stroke in &self.strokes {
            if let Some(rect) = stroke.rect {
                acc = Some(match acc {
                    None => rect,
                    Some(current) => ShapeRect {
                        tick_lo: current.tick_lo.min(rect.tick_lo),
                        key_lo: current.key_lo.min(rect.key_lo),
                        tick_hi: current.tick_hi.max(rect.tick_hi),
                        key_hi: current.key_hi.max(rect.key_hi),
                    },
                });
            }
            for anchor in &stroke.anchors {
                let (tick, key) = anchor.pos;
                let point = ShapeRect::normalized(tick, key, tick, key);
                acc = Some(match acc {
                    None => point,
                    Some(current) => ShapeRect {
                        tick_lo: current.tick_lo.min(point.tick_lo),
                        key_lo: current.key_lo.min(point.key_lo),
                        tick_hi: current.tick_hi.max(point.tick_hi),
                        key_hi: current.key_hi.max(point.key_hi),
                    },
                });
            }
        }
        acc
    }

    /// 第 `index` 条 stroke 是否闭合（有 rect，或首尾锚点重合）
    pub fn stroke_closed(&self, index: usize) -> bool {
        let Some(stroke) = self.strokes.get(index) else {
            return false;
        };
        if stroke.rect.is_some() {
            return true;
        }
        let (Some(first), Some(last)) = (stroke.anchors.first(), stroke.anchors.last()) else {
            return false;
        };
        anchor_close(first, last)
    }

    /// 逐 stroke 的角色；roles 为空时返回默认角色
    pub fn role(&self, index: usize) -> StrokeRole {
        self.roles.get(index).copied().unwrap_or_default()
    }
}

fn validate_stroke(index: usize, stroke: &Stroke) -> Result<()> {
    if stroke.anchors.len() > DrawingParams::MAX_ANCHORS {
        return Err(CoreError::InvalidArgument(format!(
            "Drawing 第 {index} 条 stroke 的锚点数超过上限（最多 {}，实际 {}）",
            DrawingParams::MAX_ANCHORS,
            stroke.anchors.len()
        )));
    }
    if stroke.anchors.is_empty() && stroke.rect.is_none() {
        return Err(CoreError::InvalidArgument(format!(
            "Drawing 第 {index} 条 stroke 既无锚点也无外接框"
        )));
    }
    if let Some(rect) = stroke.rect {
        if !rect.is_finite() {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 第 {index} 条 stroke 的外接框含非有限值"
            )));
        }
        if !rect.is_normalized() {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 第 {index} 条 stroke 的外接框未规范化"
            )));
        }
    }
    for (anchor_index, anchor) in stroke.anchors.iter().enumerate() {
        if !anchor.is_finite() {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 第 {index} 条 stroke 的第 {anchor_index} 个锚点含非有限值"
            )));
        }
        let (tick, key) = anchor.pos;
        if tick < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 第 {index} 条 stroke 的第 {anchor_index} 个锚点 tick 为负"
            )));
        }
        if key < 0.0 || key > f32::from(SHAPE_KEY_MAX) {
            return Err(CoreError::InvalidArgument(format!(
                "Drawing 第 {index} 条 stroke 的第 {anchor_index} 个锚点音高越界"
            )));
        }
    }
    Ok(())
}

/// 锚点是否可作为闭合判断点（供库/转换共用）
pub(crate) fn anchor_close(a: &ShapeAnchor, b: &ShapeAnchor) -> bool {
    let (ax, ay) = a.pos;
    let (bx, by) = b.pos;
    (ax - bx).abs() <= 1e-4 && (ay - by).abs() <= 1e-4
}
