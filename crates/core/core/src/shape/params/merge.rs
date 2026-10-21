//! Gate sensitive merge 参数（DRAW-022）
//!
//! 两个形状的音符合一：stay 不动，slide 按每个 key 行各自在时间上滑动，
//! 直到该行与 stay 同 key 行的音符相接（gate end = next start），长度不变。

use serde::{Deserialize, Serialize};

/// Merge 参数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MergeParams {
    /// 保持不动的形状 id
    pub stay: u32,
    /// 滑动方从哪一侧进入（None = 由中点判定）
    pub from_left: Option<bool>,
    /// 无对手行的 leftover 是否按最小滑移量一起滑动
    pub rest_slides: bool,
}

#[cfg(test)]
mod tests;
