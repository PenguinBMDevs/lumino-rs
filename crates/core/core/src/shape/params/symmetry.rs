//! Symmetric halves 参数（DRAW-023）
//!
//! 只存**一半**锚点与对称规则；生成前由 `shape_ops::bezier::symmetry`
//! 展开成完整锚点。走 `ShapeOp` 变体，因此不需要提升 `.lmshapes` 版本。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::Sym;

/// 对称轴
///
/// 坐标为逻辑坐标 `(tick, key)`；`Vertical` 表示 `x = 常量`，`Horizontal`
/// 表示 `y = 常量`。`Custom` 用一条过 `origin` 的无限直线表示。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SymAxis {
    /// 竖直轴：反射 tick，key 不变
    Vertical {
        /// 轴所在的 tick
        x: f32,
    },
    /// 水平轴：反射 key，tick 不变
    Horizontal {
        /// 轴所在的 key
        y: f32,
    },
    /// 自定义直线轴
    Custom {
        /// 轴上一点
        origin: (f32, f32),
        /// 轴方向（无需归一化）
        dir: (f32, f32),
    },
}

impl SymAxis {
    /// 校验轴是否可落盘/可计算
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Vertical { x } if !x.is_finite() => Err(CoreError::InvalidArgument(
                "Symmetry 竖直轴坐标必须有限".into(),
            )),
            Self::Horizontal { y } if !y.is_finite() => Err(CoreError::InvalidArgument(
                "Symmetry 水平轴坐标必须有限".into(),
            )),
            Self::Custom { origin, dir } => {
                if !origin.0.is_finite() || !origin.1.is_finite() {
                    return Err(CoreError::InvalidArgument(
                        "Symmetry 自定义轴原点必须有限".into(),
                    ));
                }
                if !dir.0.is_finite() || !dir.1.is_finite() {
                    return Err(CoreError::InvalidArgument(
                        "Symmetry 自定义轴方向必须有限".into(),
                    ));
                }
                if dir.0.abs() < f32::EPSILON && dir.1.abs() < f32::EPSILON {
                    return Err(CoreError::InvalidArgument(
                        "Symmetry 自定义轴方向不能为零".into(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Symmetry 参数
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SymParams {
    /// mirror / turn（与 Pattern 的 `Sym` 共用同一枚举）
    pub mode: Sym,
    /// 对称轴
    pub axis: SymAxis,
    /// 来源锚点起点（预留；当前为 0）
    pub source: u16,
}

impl SymParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        self.axis.validate()
    }
}

#[cfg(test)]
mod tests;
