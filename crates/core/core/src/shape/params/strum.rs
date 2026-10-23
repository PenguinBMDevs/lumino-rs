//! Strum（扫弦）参数
//!
//! 语义对应 Spiderweb `notes/strum.py`：同 tick 起音的和弦按指数级数依次错开
//! 起音/收尾/力度，可先切分（chop）再扫、可每隔一个和弦反向（alternate）。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// Strum 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrumParams {
    /// 每音错开量（拍，-4..=4；正 = 低音先）
    pub time: f32,
    /// 步进加速度（-100..=100；100 = 每步翻倍，-100 = 减半）
    pub tension: f32,
    /// 只移动起点，不改终点
    pub preserve: bool,
    /// 整体前移，使最后一个音符落在原和弦起点
    pub ahead: bool,
    /// 是否启用收尾扫
    pub end: bool,
    /// 收尾每音错开量（拍，-4..=4；正 = 最后一个保留终点，负 = 第一个）
    pub end_time: f32,
    /// 收尾步进加速度（-100..=100）
    pub end_tension: f32,
    /// 力度扫（-127..=127）
    pub vel: i16,
    /// 力度步进加速度（-100..=100）
    pub vel_tension: f32,
    /// 先按「别人的起点」切分音符
    pub chop: bool,
    /// 隔一个真实和弦反向
    pub alternate: bool,
}

impl Default for StrumParams {
    fn default() -> Self {
        Self {
            time: 0.0,
            tension: 0.0,
            preserve: false,
            ahead: false,
            end: false,
            end_time: 0.0,
            end_tension: 0.0,
            vel: 0,
            vel_tension: 0.0,
            chop: false,
            alternate: false,
        }
    }
}

impl StrumParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        let finite = [
            self.time,
            self.tension,
            self.end_time,
            self.end_tension,
            self.vel_tension,
        ];
        if finite.iter().any(|value| !value.is_finite()) {
            return Err(CoreError::InvalidArgument("Strum 参数含非有限值".into()));
        }
        if !(-4.0..=4.0).contains(&self.time) || !(-4.0..=4.0).contains(&self.end_time) {
            return Err(CoreError::InvalidArgument(format!(
                "Strum 的 time/end_time 必须在 -4..=4（实际 {} / {}）",
                self.time, self.end_time
            )));
        }
        for (name, value) in [
            ("tension", self.tension),
            ("end_tension", self.end_tension),
            ("vel_tension", self.vel_tension),
        ] {
            if !(-100.0..=100.0).contains(&value) {
                return Err(CoreError::InvalidArgument(format!(
                    "Strum 的 {name} 必须在 -100..=100（实际 {value}）"
                )));
            }
        }
        if !(-127..=127).contains(&self.vel) {
            return Err(CoreError::InvalidArgument(format!(
                "Strum 的 vel 必须在 -127..=127（实际 {}）",
                self.vel
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
