//! 填充模式与样式
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

use serde::{Deserialize, Serialize};

use super::areas::AreaSpot;
use super::{AREA_COLOUR_MAX, COLOUR_SLOT_MAX, STROKE_WIDTH_MAX, STROKE_WIDTH_MIN};
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

/// Spam 音符在门长网格上的起点对齐方式
///
/// 对应 Spiderweb `notes/custom.py` 的 `ALIGNS = (auto, aligned, centred)`。
/// 语义在 DRAW-011 落地，本卡只落字段与默认值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpamAlign {
    /// 各行自自身左边缘起铺
    #[default]
    Auto,
    /// 对齐 tick 0 的全局网格
    Aligned,
    /// 余量在两端分摊
    Centred,
}

/// 一段里放不下整数个门时，末端余数的处理方式
///
/// 对应 Spiderweb `notes/custom.py` 的
/// `ENDS = (round, keep, drop, min, stretch)`。语义在 DRAW-011 落地。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SpamEnds {
    /// ≥ 半门取整门，否则丢弃
    #[default]
    Round,
    /// 保留短音
    Keep,
    /// 丢弃（短段仍留一音）
    Drop,
    /// 同 `Drop`，但不短于 1/4 门
    Min,
    /// 整段伸缩，精确吻合
    Stretch,
}

/// 形状样式（S0 只承载字段，语义在 T1 落地）
///
/// ⚠️ v2 起本结构体**不再 `Copy`**（`areas` 为 `Vec`）。字段只能**追加到末尾**；
/// 任何字段增删/重排都必须提升 `.lmshapes` 部件版本（见 `lumino-project` 的
/// `LmshapesData`）。`ShapeOp` 变体追加不在此列——枚举末尾追加对旧数据安全。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Spam 起点对齐（DRAW-011）
    pub align: SpamAlign,
    /// 填充末端余数处理（DRAW-011）
    pub ends: SpamEnds,
    /// 重叠处也填充（取消 even-odd，DRAW-011）
    pub union: bool,
    /// 描边与内部走不同通道（DRAW-011）
    pub apart: bool,
    /// 颜色交界处也算描边（依赖区域色，DRAW-011 / DRAW-032）
    pub borders: bool,
    /// 区域着色点（DRAW-032；本卡只落类型与校验）
    pub areas: Vec<AreaSpot>,
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
            align: SpamAlign::Auto,
            ends: SpamEnds::Round,
            union: false,
            apart: false,
            borders: false,
            areas: Vec::new(),
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
        for (index, area) in self.areas.iter().enumerate() {
            area.validate().map_err(|e| {
                CoreError::InvalidArgument(format!(
                    "形状样式第 {index} 个区域着色点非法（色号上限 {AREA_COLOUR_MAX}）：{e}"
                ))
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
