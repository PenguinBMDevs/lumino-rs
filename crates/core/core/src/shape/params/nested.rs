//! Nested 内部填充参数（DRAW-017）
//!
//! 形状保留自身 Fill / Spam，内部按步进缩小的自身副本切成多块，每块一个通道。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 图形引用（本卡只支持 id=0 = 当前形状自身）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DrawingRef {
    /// 图形 id；0 = 当前形状
    pub id: u64,
}

/// 步进方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NestedWay {
    /// 轮廓逐步缩小
    #[default]
    Scale,
    /// 等厚向内收
    Band,
    /// 轮廓与中心点平滑混合
    Blend,
}

/// Nested 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedParams {
    /// 步进方式
    pub way: NestedWay,
    /// 副本数（形状自身算一步）
    pub steps: u16,
    /// 最后一个副本的尺寸 0..1
    pub last: f32,
    /// -100..100：副本向最后一个靠拢
    pub spacing: f32,
    /// 每步旋转角度（当前保留）
    pub turn: f32,
    /// 步进轮转的通道数（0 = 不轮转）
    pub colours: u8,
    /// 隔一步留空
    pub skip: bool,
    /// 缩放中心（0..1 的 box 坐标；None = 中心）
    pub at: Option<[f32; 2]>,
    /// 仅 Blend：0 = 等宽带 .. 1 = 全圆化
    pub mix: f32,
    /// 仅 Scale：副本用该图形（None = 自身）
    pub of: Option<DrawingRef>,
    /// 仅 Scale：副本绘制先转多少度
    pub angle: f32,
    /// 仅 Scale + of：从形状渐变到该图形
    pub morph: bool,
}

impl Default for NestedParams {
    fn default() -> Self {
        Self {
            way: NestedWay::Scale,
            steps: 3,
            last: 0.5,
            spacing: 0.0,
            turn: 0.0,
            colours: 0,
            skip: false,
            at: None,
            mix: 0.5,
            of: None,
            angle: 0.0,
            morph: false,
        }
    }
}

impl NestedParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        if !(1..=256).contains(&self.steps) {
            return Err(CoreError::InvalidArgument(format!(
                "Nested 的 steps 必须在 1..=256（实际 {}）",
                self.steps
            )));
        }
        if !self.last.is_finite() || !(0.0..=1.0).contains(&self.last) {
            return Err(CoreError::InvalidArgument(format!(
                "Nested 的 last 必须在 0..=1（实际 {}）",
                self.last
            )));
        }
        if !self.spacing.is_finite() || !(-100.0..=100.0).contains(&self.spacing) {
            return Err(CoreError::InvalidArgument(format!(
                "Nested 的 spacing 必须在 -100..=100（实际 {}）",
                self.spacing
            )));
        }
        if !self.turn.is_finite() || !self.angle.is_finite() {
            return Err(CoreError::InvalidArgument(
                "Nested 的 turn / angle 必须是有限数".into(),
            ));
        }
        if self.colours > 16 {
            return Err(CoreError::InvalidArgument(format!(
                "Nested 的 colours 必须在 0..=16（实际 {}）",
                self.colours
            )));
        }
        if !self.mix.is_finite() || !(0.0..=1.0).contains(&self.mix) {
            return Err(CoreError::InvalidArgument(format!(
                "Nested 的 mix 必须在 0..=1（实际 {}）",
                self.mix
            )));
        }
        if let Some(at) = self.at
            && (!at[0].is_finite()
                || !at[1].is_finite()
                || !(0.0..=1.0).contains(&at[0])
                || !(0.0..=1.0).contains(&at[1]))
        {
            return Err(CoreError::InvalidArgument(
                "Nested 的 at 必须是 0..=1 的两个有限数".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
