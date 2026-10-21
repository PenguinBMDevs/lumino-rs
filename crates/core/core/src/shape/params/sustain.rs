//! Sustain（堆叠延音）参数
//!
//! 语义对应 Spiderweb `notes/sustain.py`：同 key 行上 gap 不超过阈值的音符归为一组，
//! 组内每条延长到组末，形成「声部墙」；它是算子链的最后一步，只改长度。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 手动 stop
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StopsParams {
    /// 相对「首起点 → 末终点」区间的比例（0..=1）
    pub at: Vec<f32>,
    /// 允许 stop 落在末终点（比例 1.0）并延长最后一组
    pub past: bool,
    /// 只保留设置，不生效
    pub off: bool,
}

/// Sustain 参数
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SustainParams {
    /// 握手阈值（拍，0..=1e4）；`None` = 自动（最常见间隔 + 其半）
    pub gap_beats: Option<f32>,
    /// 单条最多延长到多长（拍，>0）；`None` = 不限
    pub most_beats: Option<f32>,
    /// 周期性 stop 的间隔（拍，>0）
    pub every_beats: Option<f32>,
    /// 周期性 stop 的起点偏移（拍，>0）
    pub from_beats: Option<f32>,
    /// 手动 stop
    pub stops: Option<StopsParams>,
}

const MAX_BEATS: f32 = 10_000.0;
const MAX_STOPS_AT: usize = 10_000;

impl SustainParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        check_beats("gap_beats", self.gap_beats, true)?;
        check_beats("most_beats", self.most_beats, false)?;
        check_beats("every_beats", self.every_beats, false)?;
        check_beats("from_beats", self.from_beats, false)?;
        if let Some(stops) = &self.stops {
            if stops.at.len() > MAX_STOPS_AT {
                return Err(CoreError::InvalidArgument(format!(
                    "Sustain 的 stops.at 不能超过 {MAX_STOPS_AT} 个（实际 {}）",
                    stops.at.len()
                )));
            }
            for (index, value) in stops.at.iter().enumerate() {
                if !value.is_finite() || !(0.0..=1.0).contains(value) {
                    return Err(CoreError::InvalidArgument(format!(
                        "Sustain 的 stops.at[{index}] 必须在 0..=1（实际 {value}）"
                    )));
                }
            }
        }
        Ok(())
    }
}

fn check_beats(name: &str, value: Option<f32>, allow_zero: bool) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    if !value.is_finite() || !(0.0..=MAX_BEATS).contains(&value) {
        return Err(CoreError::InvalidArgument(format!(
            "Sustain 的 {name} 必须在 0..={MAX_BEATS} 拍（实际 {value}）"
        )));
    }
    if !allow_zero && value == 0.0 {
        return Err(CoreError::InvalidArgument(format!(
            "Sustain 的 {name} 必须 > 0（实际 {value}）"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
