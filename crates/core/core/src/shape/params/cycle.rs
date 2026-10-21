//! Colours（通道循环）参数
//!
//! 对应 Spiderweb `custom.py` 的 `cycle`：音符按步/行/时间轮转到 n 个通道。
//! 本卡只做通道分配，不碰通道消解（DRAW-001）与 `use10`。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 轮转依据
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CycleBy {
    /// 按音符起始步（spam 门列序号）
    #[default]
    Step,
    /// 按 key 行
    Key,
    /// 按时间
    Time,
}

/// 每 key 行独立起算的方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowRestart {
    /// 从该行首音符起算
    First,
    /// 从该行末音符倒算
    End,
}

/// Colours 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleParams {
    /// 轮转依据
    pub by: CycleBy,
    /// 轮转几个通道（2..=16）
    pub n: u8,
    /// step/key：每多少步/行换一次
    pub every: u32,
    /// time：步长 a/b 个全音符（如 [1,4] = 一拍）
    pub every_time: Option<[u32; 2]>,
    /// step/time：每 key 行独立起算
    pub rows: Option<RowRestart>,
    /// step/time：对角 [s, m]（每 m 行轮转起点后移 s 个通道）
    pub shift: Option<[i32; 2]>,
}

impl Default for CycleParams {
    fn default() -> Self {
        Self {
            by: CycleBy::Step,
            n: 2,
            every: 1,
            every_time: Some([1, 4]),
            rows: None,
            shift: None,
        }
    }
}

impl CycleParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝）
    pub fn validate(&self) -> Result<()> {
        if !(2..=16).contains(&self.n) {
            return Err(CoreError::InvalidArgument(format!(
                "Cycle 的 n 必须在 2..=16（实际 {}）",
                self.n
            )));
        }
        if !(1..=10_000).contains(&self.every) {
            return Err(CoreError::InvalidArgument(format!(
                "Cycle 的 every 必须在 1..=10000（实际 {}）",
                self.every
            )));
        }
        if let Some([a, b]) = self.every_time
            && (a == 0 || b == 0 || a > 10_000 || b > 10_000)
        {
            return Err(CoreError::InvalidArgument(format!(
                "Cycle 的 every_time 必须是 [a,b]（1..=10000，实际 [{a},{b}]）"
            )));
        }
        if let Some([s, m]) = self.shift {
            if !(1..=10_000).contains(&m) {
                return Err(CoreError::InvalidArgument(format!(
                    "Cycle 的 shift.m 必须在 1..=10000（实际 {m}）"
                )));
            }
            if !(-10_000..=10_000).contains(&s) {
                return Err(CoreError::InvalidArgument(format!(
                    "Cycle 的 shift.s 必须在 -10000..=10000（实际 {s}）"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
