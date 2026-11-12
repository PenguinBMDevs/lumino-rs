//! RoundCorner 参数（DRAW-044）
//!
//! 角度柔化：按锚点索引逐角点存圆角半径（key 单位）。0 = 该角保持硬折。
//! 实际圆弧每次重算，因此调小半径可以逐点回到原折角。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 单个角的圆角
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoundCorner {
    /// 锚点索引（原路径上的角点）
    pub index: u16,
    /// 圆角半径（key 单位；0 = 不柔化）
    pub radius: f32,
}

/// RoundCorner 参数
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RoundParams {
    /// 逐角点圆角（同一下标以最后一条为准）
    #[serde(default)]
    pub corners: Vec<RoundCorner>,
    /// 柔化时冻结的「每 key 几拍」（beats per key）
    ///
    /// 圆弧要在屏幕上保持正圆，x（tick）必须折到与 key 同尺度。创建时用当时
    /// 视图的比例冻结，之后缩放视图不再改变形状；`0` = 回退到模型自身的 `k`。
    #[serde(default)]
    pub k: f32,
    /// 柔化时冻结的工程 PPQ（0 = 未冻结，回退到调用方传入的当前 PPQ）
    ///
    /// `k` 是「拍/key」，tick→key 还需要「tick/拍」。工程 PPQ 可以中途修改
    ///（`Root::set_ppq` 不会重排已有音符），若实时读取，已柔化的形状会在改 PPQ
    /// 后变形。冻结后几何只由创建时的视图决定，保存重开也稳定。
    #[serde(default)]
    pub ppq: u16,
}

impl RoundParams {
    /// 校验：半径必须是有限非负数
    pub fn validate(&self) -> Result<()> {
        for corner in &self.corners {
            if !corner.radius.is_finite() || corner.radius < 0.0 {
                return Err(CoreError::InvalidArgument(format!(
                    "Round 的半径必须是有限非负数（index {} 实际 {}）",
                    corner.index, corner.radius
                )));
            }
        }
        Ok(())
    }

    /// 取某角点的半径（同一下标以最后一条为准；未登记返回 `None`）
    pub fn radius_of(&self, index: usize) -> Option<f32> {
        self.corners
            .iter()
            .rev()
            .find(|corner| usize::from(corner.index) == index)
            .map(|corner| corner.radius)
    }

    /// 设置某角点的半径（覆盖同下标；`radius <= 0` 表示移除该角）
    pub fn set_radius(&mut self, index: usize, radius: f32) {
        let index = index.min(u16::MAX as usize) as u16;
        self.corners.retain(|corner| corner.index != index);
        if radius > 0.0 {
            self.corners.push(RoundCorner { index, radius });
        }
    }

    /// 是否没有任何角点被柔化
    pub fn is_empty(&self) -> bool {
        self.corners.iter().all(|corner| corner.radius <= 0.0)
    }

    /// 几何求解用的 beats-per-key：优先用冻结值，否则回退到模型 `k`
    pub fn effective_k(&self, model_k: f32) -> f32 {
        if self.k.is_finite() && self.k > 0.0 {
            self.k
        } else if model_k.is_finite() && model_k > 0.0 {
            model_k
        } else {
            1.0
        }
    }

    /// 几何求解用的 PPQ：优先用冻结值，否则回退到调用方的当前 PPQ
    pub fn effective_ppq(&self, current_ppq: u16) -> u16 {
        if self.ppq == 0 {
            current_ppq.max(1)
        } else {
            self.ppq
        }
    }
}

#[cfg(test)]
mod tests;
