//! Mask（遮罩）参数
//!
//! 语义对应 Spiderweb `notes/mask.py`：遮罩只影响显示——被遮形状的音符整条保留、
//! 照常发声，只在遮罩外画成 ghost；`Apply mask` 才把可见音符实体化。
//! 本模块只承载参数；区域代数与组合在 `shape_ops::masks`。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 遮罩区域来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MaskShow {
    /// 闭合形状取内部（默认）
    #[default]
    Inside,
    /// 只取沿线带
    Line,
}

/// 同一 group 内的组合方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MaskMode {
    /// 并入（组内首个成员恒为 Add）
    #[default]
    Add,
    /// 从当前区域减去
    Subtract,
    /// 与当前区域相交
    Intersect,
}

/// Mask 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaskParams {
    /// 带宽（keys，0..=127）；按 `k` 换算，跨 zoom 圆度不变
    pub size_keys: f32,
    /// 视图比例（beats per key，>0）；遮罩成为遮罩时的屏幕比例
    pub k: f32,
    /// 取反：图案只显示在遮罩之外（挖洞）
    pub invert: bool,
    /// 自身音符发声（边框）；`false` = 纯切割，只显示虚线
    pub own: bool,
    /// 区域来源
    pub show: MaskShow,
    /// 组合组 id（None = 独立遮罩）
    pub group: Option<u32>,
    /// 组内组合方式
    pub mode: MaskMode,
    /// 组合虚线颜色索引（0..=5）
    pub colour: u8,
}

impl Default for MaskParams {
    fn default() -> Self {
        Self {
            size_keys: 2.0,
            k: 0.0625,
            invert: false,
            own: true,
            show: MaskShow::Inside,
            group: None,
            mode: MaskMode::Add,
            colour: 0,
        }
    }
}

impl MaskParams {
    /// 组颜色上限（上游 `GROUP_COLOURS = 6`）
    pub const GROUP_COLOURS: u8 = 6;

    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        if !self.size_keys.is_finite() || !(0.0..=127.0).contains(&self.size_keys) {
            return Err(CoreError::InvalidArgument(format!(
                "Mask 的 size_keys 必须在 0..=127（实际 {}）",
                self.size_keys
            )));
        }
        if !self.k.is_finite() || !(0.0..=10_000.0).contains(&self.k) || self.k == 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Mask 的 k 必须在 (0, 10000]（实际 {}）",
                self.k
            )));
        }
        if let Some(group) = self.group
            && !(1..=999_999_999).contains(&group)
        {
            return Err(CoreError::InvalidArgument(format!(
                "Mask 的 group 必须在 1..=999999999（实际 {group}）"
            )));
        }
        if self.colour >= Self::GROUP_COLOURS {
            return Err(CoreError::InvalidArgument(format!(
                "Mask 的 colour 必须在 0..{}（实际 {}）",
                Self::GROUP_COLOURS - 1,
                self.colour
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
