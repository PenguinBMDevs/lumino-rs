//! Claw machine（爪机）参数
//!
//! 语义对应 Spiderweb `notes/claw.py`：按时间片丢弃/保留音符、按计数抽稀、
//! 按比例随机抽稀；可收紧时长或把被丢弃的音符改短。
//! 本卡只落参数与算法，不含面板 UI。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 抽稀依据
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClawMode {
    /// 按时间片（period）丢弃/保留
    #[default]
    Time,
    /// 按 (start, key) 排序后的音符序号
    Notes,
    /// 从最低 key 起数（无音符的 key 也占位）
    Keys,
    /// 同 tick 起音算一个 chord
    Chords,
    /// 按 seed 伪随机
    Random,
}

/// time 模式的分片周期（拍）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ClawPeriod {
    /// 1 拍
    One,
    /// 2 拍
    Two,
    /// 4 拍
    #[default]
    Four,
    /// 8 拍
    Eight,
    /// 16 拍
    Sixteen,
}

impl ClawPeriod {
    /// 周期（拍）
    pub fn beats(self) -> f32 {
        match self {
            Self::One => 1.0,
            Self::Two => 2.0,
            Self::Four => 4.0,
            Self::Eight => 8.0,
            Self::Sixteen => 16.0,
        }
    }
}

/// time 模式的丢弃片：每段切成 `n` 片，丢掉第 `k` 片（0 基）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawTrash {
    /// 片索引（0 基，必须 < n）
    pub k: u16,
    /// 每段切成的片数（>= 1）
    pub n: u16,
}

/// Claw 参数
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawParams {
    /// 抽稀依据
    pub mode: ClawMode,
    /// time 模式周期
    pub period: ClawPeriod,
    /// time 模式丢弃片；`None` = 整段丢弃
    pub trash: Option<ClawTrash>,
    /// 段内时间弯曲（-100..=100；正 = 先长后短）
    pub dist: i8,
    /// 压掉空隙后拉回原总长
    pub stretch: bool,
    /// 删除爪机造成的新短音（原本就短的保留）
    pub short: bool,
    /// 计数模式：每个周期的保留数（1..=1000）
    pub keep: u16,
    /// 计数模式：每个周期的跳过数（0..=1000）
    pub skip: u16,
    /// random 模式保留率（0..=100）
    pub pct: u8,
    /// random 模式种子（<= 1e9，可复现）
    pub seed: u32,
    /// 计数/random 模式：被丢弃的音符改短保留
    pub shorten: bool,
    /// 改短后的长度百分比（1..=99）
    pub cut: u8,
}

impl Default for ClawParams {
    fn default() -> Self {
        Self {
            mode: ClawMode::Time,
            period: ClawPeriod::Four,
            trash: Some(ClawTrash { k: 1, n: 4 }),
            dist: 0,
            stretch: false,
            short: false,
            keep: 1,
            skip: 0,
            pct: 50,
            seed: 0,
            shorten: false,
            cut: 50,
        }
    }
}

impl ClawParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        if let Some(trash) = self.trash {
            if trash.n == 0 {
                return Err(CoreError::InvalidArgument(
                    "Claw 的 trash.n 必须 >= 1".into(),
                ));
            }
            if trash.k >= trash.n {
                return Err(CoreError::InvalidArgument(format!(
                    "Claw 的 trash.k 必须 < n（实际 k={}, n={}）",
                    trash.k, trash.n
                )));
            }
        }
        if !(-100..=100).contains(&self.dist) {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 dist 必须在 -100..=100（实际 {}）",
                self.dist
            )));
        }
        if self.keep == 0 || self.keep > 1000 {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 keep 必须在 1..=1000（实际 {}）",
                self.keep
            )));
        }
        if self.skip > 1000 {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 skip 必须在 0..=1000（实际 {}）",
                self.skip
            )));
        }
        if self.pct > 100 {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 pct 必须在 0..=100（实际 {}）",
                self.pct
            )));
        }
        if self.seed > 1_000_000_000 {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 seed 必须 <= 1e9（实际 {}）",
                self.seed
            )));
        }
        if self.cut == 0 || self.cut > 99 {
            return Err(CoreError::InvalidArgument(format!(
                "Claw 的 cut 必须在 1..=99（实际 {}）",
                self.cut
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
