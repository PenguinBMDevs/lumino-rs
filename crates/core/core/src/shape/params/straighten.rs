//! Straighten 参数（DRAW-024）
//!
//! 手绘扶正只存灵敏度 `level`（0..=100）；0 表示原样。实际拟合每次重算，
//! 因此调低灵敏度可以逐点回到原绘制。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Straighten 参数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StraightenParams {
    /// 灵敏度 0..=100；0 = 原样
    pub level: u8,
}

impl StraightenParams {
    /// 校验灵敏度
    pub fn validate(&self) -> Result<()> {
        if self.level > 100 {
            return Err(CoreError::InvalidArgument(format!(
                "Straighten 的 level 必须在 0..=100（实际 {}）",
                self.level
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
