//! 形状区域着色点（DRAW-032 的数据底座）
//!
//! 本模块只承载**类型与校验**；区域求解与着色算法在 DRAW-032 落地。
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

use serde::{Deserialize, Serialize};

use super::AREA_COLOUR_MAX;
use crate::error::{CoreError, Result};

/// 形状内部的一个着色点
///
/// 对应 Spiderweb `notes/areas.py` 的 `sh["areas"] = [[u, v, colour]]`：
/// `(u, v)` 是形状自身 box 内的**归一化坐标**（`0..=1`），`colour`：
/// - `0` = 空（不填色）
/// - `1..=15` = 颜色槽
/// - `16` = 通道 10（仅在工程启用通道 10 时可用；本类型不判断工程状态）
///
/// `u` / `v` 随形状移动而移动，因此存**比例**而非绝对 tick/key——
/// 形状平移后着色点必须跟着走（与 DRAW-007 的框选坐标同一口径）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AreaSpot {
    /// box 内归一化横坐标（0..=1）
    pub u: f32,
    /// box 内归一化纵坐标（0..=1）
    pub v: f32,
    /// 色号（0..=16；16 需工程启用通道 10）
    pub colour: u8,
}

impl AreaSpot {
    /// 校验单个着色点：坐标有限且落在 box 内、色号不越界
    ///
    /// 返回 `Err` 而不是 panic：手改/损坏的工程文件必须能被拒绝并给出可读原因。
    pub fn validate(&self) -> Result<()> {
        if !self.u.is_finite() || !self.v.is_finite() {
            return Err(CoreError::InvalidArgument(
                "区域着色点坐标含非有限值".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.u) || !(0.0..=1.0).contains(&self.v) {
            return Err(CoreError::InvalidArgument(format!(
                "区域着色点坐标越界（允许 0..=1，实际 u={}, v={}）",
                self.u, self.v
            )));
        }
        if self.colour > AREA_COLOUR_MAX {
            return Err(CoreError::InvalidArgument(format!(
                "区域色号越界（允许 0..={}，实际 {}）",
                AREA_COLOUR_MAX, self.colour
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
