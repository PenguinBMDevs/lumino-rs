//! Funnel 最简参数（DRAW-018）
//!
//! 本卡只支持：1 条线 + 2 个墙端 + 直线端 + `Empty` / `Spam`。
//! 多线、曲线端、follow:curve、Hz 层留给后续卡。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::{SHAPE_KEY_MAX, ShapeFill};

/// 曲线端占位（本卡按直线处理；存控制点供后续卡扩展）
pub type CurveRef = Vec<[f32; 2]>;

/// 一个起点：从线上 `at` 处向两个墙端各开一条端曲线
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunnelStart {
    /// 线序号（本卡恒 0）
    pub line: u8,
    /// 在线上的位置（0..=1）
    pub at: f32,
    /// 到两个墙端的曲线（本卡忽略，按直线）
    pub ends: [Option<CurveRef>; 2],
}

/// Funnel 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunnelParams {
    /// 线（本卡恰好一条；`[起点, 终点]`）
    pub lines: Vec<[[f32; 2]; 2]>,
    /// 墙的两个端点
    pub wall: [[f32; 2]; 2],
    /// 起点列表（空 = 线起点直接到墙）
    pub starts: Vec<FunnelStart>,
    /// 填充：本卡 Empty / Spam
    pub fill: ShapeFill,
    /// 墙在时间上先于线 ⇒ 反向漏斗
    pub reverse: bool,
}

impl FunnelParams {
    /// 校验参数（加载坏工程文件不得 panic）
    pub fn validate(&self) -> Result<()> {
        if self.lines.len() != 1 {
            return Err(CoreError::InvalidArgument(format!(
                "Funnel 本卡仅支持 1 条线（实际 {}）",
                self.lines.len()
            )));
        }
        for (index, line) in self.lines.iter().enumerate() {
            for (end, point) in line.iter().enumerate() {
                validate_point(*point, &format!("lines[{index}][{end}]"))?;
            }
        }
        for (index, point) in self.wall.iter().enumerate() {
            validate_point(*point, &format!("wall[{index}]"))?;
        }
        for (index, start) in self.starts.iter().enumerate() {
            if start.line != 0 {
                return Err(CoreError::InvalidArgument(format!(
                    "Funnel starts[{index}].line 必须为 0（实际 {}）",
                    start.line
                )));
            }
            if !start.at.is_finite() || !(0.0..=1.0).contains(&start.at) {
                return Err(CoreError::InvalidArgument(format!(
                    "Funnel starts[{index}].at 必须在 0..=1（实际 {}）",
                    start.at
                )));
            }
            for (end, curve) in start.ends.iter().enumerate() {
                if let Some(points) = curve {
                    for point in points {
                        validate_point(*point, &format!("starts[{index}].ends[{end}]"))?;
                    }
                }
            }
        }
        if !matches!(self.fill, ShapeFill::Empty | ShapeFill::Spam) {
            return Err(CoreError::InvalidArgument(
                "Funnel 本卡只支持 Empty / Spam 填充".into(),
            ));
        }
        Ok(())
    }
}

fn validate_point(point: [f32; 2], label: &str) -> Result<()> {
    if !point[0].is_finite() || !point[1].is_finite() {
        return Err(CoreError::InvalidArgument(format!(
            "Funnel {label} 含非有限值"
        )));
    }
    if point[0] < 0.0 {
        return Err(CoreError::InvalidArgument(format!(
            "Funnel {label} 的 tick 为负（实际 {}）",
            point[0]
        )));
    }
    if point[1] < 0.0 || point[1] > f32::from(SHAPE_KEY_MAX) {
        return Err(CoreError::InvalidArgument(format!(
            "Funnel {label} 的 key 越界（允许 0..={}，实际 {}）",
            SHAPE_KEY_MAX, point[1]
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
