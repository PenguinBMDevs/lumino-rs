//! Join 参数（DRAW-021）
//!
//! 把多个形状串成一条 Curve；`gaps` 段不绘制/不产音符，`splits` 记录切点，
//! `src` 保留每段来源 id，`tumours` 逐段保留凸起设置。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::TumourParams;

/// Joined 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinedParams {
    /// 空隙的段号（不绘制、不产音符）
    pub gaps: Vec<u16>,
    /// 每段凸起设置（None = 无）
    pub tumours: Vec<Option<TumourParams>>,
    /// 中断点所在锚点序号（新形状从这里开始）
    pub splits: Vec<u16>,
    /// 每段来源形状 id
    pub src: Vec<u32>,
}

impl JoinedParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        if self.src.is_empty() {
            return Err(CoreError::InvalidArgument("Joined 至少需要一段".into()));
        }
        if self.tumours.len() != self.src.len() {
            return Err(CoreError::InvalidArgument(
                "Joined 的 tumours 必须与 src 等长".into(),
            ));
        }
        if self.gaps.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(CoreError::InvalidArgument(
                "Joined 的 gaps 必须严格递增".into(),
            ));
        }
        if self.splits.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(CoreError::InvalidArgument(
                "Joined 的 splits 必须严格递增".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
