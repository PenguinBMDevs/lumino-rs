//! Glue（形状级粘合）参数
//!
//! 语义对应 Spiderweb `notes/glue.py`：形状保持原样，只在生成音符时把
//! 同 key、同 track 上相接/重叠的音符粘成一条长音。它是 Notes 阶段的**第一步**。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 粘合范围
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum GlueParams {
    /// 形状全部音符参与
    #[default]
    All,
    /// 按形状自身包围盒的比例框选：`[u0, v0, u1, v1]`，四个分量均在 0..=1
    ///
    /// `u` 沿时间轴（左→右），`v` 沿音高（低→高）；角点顺序可颠倒，使用时会归一化。
    Boxes(Vec<[f32; 4]>),
}

impl GlueParams {
    /// 校验框选参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        let Self::Boxes(boxes) = self else {
            return Ok(());
        };
        for (index, corners) in boxes.iter().enumerate() {
            if corners.iter().any(|value| !value.is_finite()) {
                return Err(CoreError::InvalidArgument(format!(
                    "Glue 的 Boxes 第 {index} 个框含非有限值"
                )));
            }
            if corners.iter().any(|value| !(0.0..=1.0).contains(value)) {
                return Err(CoreError::InvalidArgument(format!(
                    "Glue 的 Boxes 第 {index} 个框越界（允许 0..=1，实际 {corners:?}）"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
