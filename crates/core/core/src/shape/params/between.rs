//! Add between 参数（DRAW-026）
//!
//! 首/末形状由用户画，Step 由 `blend` 派生；首/末变化时 `sync_groups` 重算。
//! `First` 持有组设置，`Step` 持有制作指纹；手改后的 Step 降级为 Key。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 默认步数
pub const STEPS_DEFAULT: u16 = 10;
/// 步数上限
pub const MOST_STEPS: u16 = 500;
/// 颜色上限
pub const COLOURS_MOST: u8 = 16;

/// 组内角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetweenRole {
    /// 首形状
    First,
    /// 末形状
    Last,
    /// 派生步
    Step,
    /// 手改后冻结的步
    Key,
}

/// 组设置（仅 First 持有）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetweenSet {
    /// 步数
    pub steps: u16,
    /// 插值曲线 `[[u, f], ...]`；空 = 线性
    pub graph: Vec<[f32; 2]>,
    /// 末形状点反序配对
    pub rev: bool,
    /// 自动分色 0 = 关
    pub colours: u8,
    /// 跨步无角
    pub smooth: bool,
}

impl Default for BetweenSet {
    fn default() -> Self {
        Self {
            steps: STEPS_DEFAULT,
            graph: Vec::new(),
            rev: false,
            colours: 0,
            smooth: false,
        }
    }
}

impl BetweenSet {
    /// 校验组设置
    pub fn validate(&self) -> Result<()> {
        if self.steps == 0 {
            return Err(CoreError::InvalidArgument(
                "Between 的 steps 必须大于 0".into(),
            ));
        }
        if self.steps > MOST_STEPS {
            return Err(CoreError::InvalidArgument(format!(
                "Between 的 steps 不能超过 {MOST_STEPS}"
            )));
        }
        if self.colours > COLOURS_MOST {
            return Err(CoreError::InvalidArgument(format!(
                "Between 的 colours 不能超过 {COLOURS_MOST}"
            )));
        }
        for point in &self.graph {
            if !point[0].is_finite() || !point[1].is_finite() {
                return Err(CoreError::InvalidArgument(
                    "Between 的 graph 含非有限值".into(),
                ));
            }
        }
        Ok(())
    }
}

/// 单个形状的 Between 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetweenParams {
    /// 组 id
    pub id: u64,
    /// 角色
    pub role: BetweenRole,
    /// Step/Key 在组内的位置（0 = 首，1 = 末）
    pub at: Option<f32>,
    /// Step 的制作指纹；不匹配则降级为 Key
    pub sig: Option<u64>,
    /// 首/末/Key 的离开方向与力度
    pub push: Option<[f32; 2]>,
    /// 仅改过力度的 step（vel0, vel1）
    pub vel: Option<[f32; 2]>,
    /// 仅 First 持有组设置
    pub set: Option<BetweenSet>,
}

impl BetweenParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        if let Some(at) = self.at
            && (!at.is_finite() || !(0.0..=1.0).contains(&at))
        {
            return Err(CoreError::InvalidArgument(
                "Between 的 at 必须在 0..=1".into(),
            ));
        }
        if let Some(set) = &self.set {
            set.validate()?;
        }
        if matches!(self.role, BetweenRole::First) && self.set.is_none() {
            return Err(CoreError::InvalidArgument(
                "Between 的 First 必须持有组设置".into(),
            ));
        }
        if matches!(self.role, BetweenRole::Step) && self.at.is_none() {
            return Err(CoreError::InvalidArgument(
                "Between 的 Step 必须有 at".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
