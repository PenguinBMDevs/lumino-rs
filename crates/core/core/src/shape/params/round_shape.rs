//! 角度柔化阶段 2：逐角点弧形状（DEBT-18）
//!
//! `Round` 决定切点位置（半径），本模块决定两切点之间的曲线形状：
//! - `0` = 正圆弧（与阶段 1 完全同源，几何不进入本模块的 Bezier 分支）；
//! - `> 0` = 更鼓（向角点方向凸出）；
//! - `< 0` = 更平（向弦线方向收敛）。
//!
//! # 为什么独立成一个算子，而不是给 `RoundParams` 加字段
//!
//! `.lmshapes` 是 bincode **位置编码**：给 `RoundCorner` / `RoundParams` 加字段
//! 会让 v3 工程解码错位，必须提升部件版本并保留整棵 v3 快照。而 `ShapeOp`
//! 的既有纪律是「变体只能追加到末尾，追加变体不需要提版本」——旧数据不会出现
//! 新变体索引。因此形状参数以 `ShapeOp::RoundShape` 追加：v3 工程原样可读，
//! 无形状参数的工程加载后就是正圆弧（与阶段 1 逐音符一致）。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 形状参数下限（负值 = 更平；-0.9 时控制柄只有圆角的 10%）
pub const MIN_SHAPE: f32 = -0.9;
/// 形状参数上限（几何侧还会把控制柄长度再夹到切点距离，避免越过角点）
pub const MAX_SHAPE: f32 = 1.0;

/// 单个角点的弧形状
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RoundShape {
    /// 锚点索引（与 `RoundCorner.index` 同一个下标空间）
    pub index: u16,
    /// 形状参数（0 = 正圆弧；见模块文档）
    pub shape: f32,
}

/// 逐角点弧形状参数
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RoundShapeParams {
    /// 逐角点形状（同一下标以最后一条为准）
    #[serde(default)]
    pub shapes: Vec<RoundShape>,
}

impl RoundShapeParams {
    /// 把任意输入夹到合法区间（非有限值 → 0，即正圆弧）
    pub fn clamp_shape(shape: f32) -> f32 {
        if !shape.is_finite() {
            0.0
        } else {
            shape.clamp(MIN_SHAPE, MAX_SHAPE)
        }
    }

    /// 校验：形状必须有限且落在 `[MIN_SHAPE, MAX_SHAPE]`
    pub fn validate(&self) -> Result<()> {
        for entry in &self.shapes {
            if !entry.shape.is_finite() || !(MIN_SHAPE..=MAX_SHAPE).contains(&entry.shape) {
                return Err(CoreError::InvalidArgument(format!(
                    "RoundShape 的形状必须在 [{MIN_SHAPE}, {MAX_SHAPE}]（index {} 实际 {}）",
                    entry.index, entry.shape
                )));
            }
        }
        Ok(())
    }

    /// 取某角点的形状（同一下标以最后一条为准；未登记返回 `None`）
    pub fn shape_of(&self, index: usize) -> Option<f32> {
        self.shapes
            .iter()
            .rev()
            .find(|entry| usize::from(entry.index) == index)
            .map(|entry| entry.shape)
    }

    /// 设置某角点的形状（覆盖同下标；`0` / 非有限值表示移除该角）
    pub fn set_shape(&mut self, index: usize, shape: f32) {
        let index = index.min(u16::MAX as usize) as u16;
        self.shapes.retain(|entry| entry.index != index);
        let shape = Self::clamp_shape(shape);
        if shape.abs() > 1e-6 {
            self.shapes.push(RoundShape { index, shape });
        }
    }

    /// 是否没有任何角点使用自定义形状（等价于全部正圆弧）
    pub fn is_empty(&self) -> bool {
        self.shapes.iter().all(|entry| entry.shape.abs() <= 1e-6)
    }
}

#[cfg(test)]
mod tests;
