//! Chop（节奏切分）参数
//!
//! 语义对应 Spiderweb `notes/chop.py`：8 个内置节奏 + 自绘节奏、百分比/固定力度、
//! 绝对网格对齐。本卡只做算法，不含自绘节奏编辑器。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 一个节奏片（起点/长度以「步」为单位，`vel` 为百分比或绝对值）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChopPiece {
    /// 在重复周期内的起点（步）
    pub start: f32,
    /// 长度（步；0 = 1 tick）
    pub len: f32,
    /// 力度（百分比 0..=100，或 fixed 模式下的绝对值 0..=127）
    pub vel: u8,
}

/// Chop 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChopParams {
    /// 内置节奏名（仅回显；算法只用下方数值）
    pub name: String,
    /// 一次重复占几步（1..=64）
    pub steps: u8,
    /// 一个重复内的片（<= 256）
    pub pieces: Vec<ChopPiece>,
    /// 一步的时长（拍；0 = 1 tick）
    pub len: f32,
    /// 从 tick 0 对齐（否则从音符自身起点）
    pub abs: bool,
    /// 节奏力度权重（0..=100）
    pub vel: u8,
    /// 片力度用绝对值（1..=127）而非百分比
    pub fixed: bool,
}

impl Default for ChopParams {
    fn default() -> Self {
        Self::builtin("even").unwrap_or_else(|| Self {
            name: String::new(),
            steps: 1,
            pieces: Vec::new(),
            len: 1.0,
            abs: false,
            vel: 100,
            fixed: false,
        })
    }
}

impl ChopParams {
    /// 8 个内置节奏名
    pub const BUILTIN_NAMES: [&'static str; 8] = [
        "even", "half", "swing", "offbeat", "gallop", "triplets", "accent", "stutter",
    ];

    /// 按名字构造内置节奏；未知名字返回 `None`
    pub fn builtin(name: &str) -> Option<Self> {
        let pieces: Vec<ChopPiece> = match name {
            "even" => vec![(0.0, 1.0, 100)],
            "half" => vec![(0.0, 0.5, 100)],
            "swing" => vec![(0.0, 4.0 / 3.0, 100), (4.0 / 3.0, 2.0 / 3.0, 75)],
            "offbeat" => vec![(1.0, 1.0, 100)],
            "gallop" => vec![(0.0, 1.0, 100), (1.0, 0.5, 80), (1.5, 0.5, 80)],
            "triplets" => vec![
                (0.0, 1.0 / 3.0, 100),
                (1.0 / 3.0, 1.0 / 3.0, 80),
                (2.0 / 3.0, 1.0 / 3.0, 80),
            ],
            "accent" => vec![
                (0.0, 1.0, 100),
                (1.0, 1.0, 65),
                (2.0, 1.0, 65),
                (3.0, 1.0, 65),
            ],
            "stutter" => vec![
                (0.0, 0.5, 100),
                (0.5, 0.5, 80),
                (1.0, 0.5, 100),
                (1.5, 0.5, 80),
                (2.0, 2.0, 100),
            ],
            _ => return None,
        }
        .into_iter()
        .map(|(start, len, vel)| ChopPiece { start, len, vel })
        .collect();
        let steps = match name {
            "swing" | "offbeat" | "gallop" => 2,
            "accent" | "stutter" => 4,
            _ => 1,
        };
        Some(Self {
            name: name.to_string(),
            steps,
            pieces,
            len: 1.0,
            abs: false,
            vel: 100,
            fixed: false,
        })
    }

    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        if self.steps == 0 || self.steps > 64 {
            return Err(CoreError::InvalidArgument(format!(
                "Chop 的 steps 必须在 1..=64（实际 {}）",
                self.steps
            )));
        }
        if self.pieces.len() > 256 {
            return Err(CoreError::InvalidArgument(format!(
                "Chop 的 pieces 不能超过 256（实际 {}）",
                self.pieces.len()
            )));
        }
        if !self.len.is_finite() || !(0.0..=10_000.0).contains(&self.len) {
            return Err(CoreError::InvalidArgument(format!(
                "Chop 的 len 必须在 0..=10000 拍（实际 {}）",
                self.len
            )));
        }
        if self.vel > 100 {
            return Err(CoreError::InvalidArgument(format!(
                "Chop 的 vel 必须在 0..=100（实际 {}）",
                self.vel
            )));
        }
        for (index, piece) in self.pieces.iter().enumerate() {
            if !piece.start.is_finite() || !piece.len.is_finite() {
                return Err(CoreError::InvalidArgument(format!(
                    "Chop 第 {index} 个片含非有限值"
                )));
            }
            if piece.start < 0.0 || piece.len < 0.0 {
                return Err(CoreError::InvalidArgument(format!(
                    "Chop 第 {index} 个片的 start/len 不能为负（实际 {} / {}）",
                    piece.start, piece.len
                )));
            }
            if piece.vel > 127 {
                return Err(CoreError::InvalidArgument(format!(
                    "Chop 第 {index} 个片的 vel 越界（允许 0..=127，实际 {}）",
                    piece.vel
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
