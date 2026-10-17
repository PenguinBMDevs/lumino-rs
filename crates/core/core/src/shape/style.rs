//! 填充模式与样式
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

use serde::{Deserialize, Serialize};

use super::{COLOUR_SLOT_MAX, STROKE_WIDTH_MAX, STROKE_WIDTH_MIN};
use crate::error::{CoreError, Result};

/// 填充模式
///
/// 对应 Spiderweb `notes/custom.py` 的 `FILLS`。S0 只承载取值，
/// 语义（门长切片、末端策略、轮廓独立通道）在 T1 落地。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ShapeFill {
    /// 只描边，不填充内部
    #[default]
    Empty,
    /// 内部每键每段一条长音
    Fill,
    /// 内部按门长切片（背靠背）
    Spam,
    /// 只把描边按门长切片
    OutlineSpam,
}

impl ShapeFill {
    /// 是否需要门长参数
    pub fn needs_gate(self) -> bool {
        matches!(self, Self::Spam | Self::OutlineSpam)
    }
}

/// 形状样式（S0 只承载字段，语义在 T1 落地）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShapeStyle {
    /// 填充模式
    pub fill: ShapeFill,
    /// Spam 门长（**拍**，与 PPQ 无关；落地时乘 PPQ）
    pub gate_beats: f32,
    /// 描边宽度（key）
    pub stroke_width: u8,
    /// 基础力度（0..=127）
    pub velocity: u8,
    /// 颜色槽（0..=15）
    pub colour_slot: u8,
}

impl Default for ShapeStyle {
    fn default() -> Self {
        Self {
            fill: ShapeFill::Empty,
            // 与 Spiderweb `CUSTOM_DEFAULTS["gate"]` 一致：1/16 拍
            gate_beats: 0.0625,
            stroke_width: 1,
            velocity: 100,
            colour_slot: 0,
        }
    }
}

impl ShapeStyle {
    /// 样式字段是否合法（不含几何校验）
    pub fn validate(&self) -> Result<()> {
        if !self.gate_beats.is_finite() || self.gate_beats <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "形状样式门长非法（需有限且为正，实际 {}）",
                self.gate_beats
            )));
        }
        if !(STROKE_WIDTH_MIN..=STROKE_WIDTH_MAX).contains(&self.stroke_width) {
            return Err(CoreError::InvalidArgument(format!(
                "描边宽度越界（允许 {}..={}，实际 {}）",
                STROKE_WIDTH_MIN, STROKE_WIDTH_MAX, self.stroke_width
            )));
        }
        if self.velocity > 127 {
            return Err(CoreError::InvalidArgument(format!(
                "力度越界（允许 0..=127，实际 {}）",
                self.velocity
            )));
        }
        if self.colour_slot > COLOUR_SLOT_MAX {
            return Err(CoreError::InvalidArgument(format!(
                "颜色槽越界（允许 0..={}，实际 {}）",
                COLOUR_SLOT_MAX, self.colour_slot
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
