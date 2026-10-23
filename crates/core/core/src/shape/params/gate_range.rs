//! Gate Range（门控范围）参数
//!
//! 对应 Spiderweb `gaterange.py`：spam 的门长沿形状从 `gate_beats` 渐变到 `to`，
//! 走用户 graph。第一个门长复用 `ShapeStyle.gate_beats`，本结构不重复存。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 渐变方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RangeDir {
    /// 沿时间（左→右）
    #[default]
    Time,
    /// 沿 key 行（低→高）
    Keys,
    /// 沿 key 行（高→低；等价 keys + 反向 graph）
    KeysDown,
}

/// Gate Range 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateRangeParams {
    /// 第二个门长（拍，>0，≤1e4）
    pub to: f32,
    /// graph：`[[u, y]]`，u/y 均 0..1，首 u=0、末 u=1
    pub graph: Vec<[f32; 2]>,
    /// 方向
    pub dir: RangeDir,
    /// 首末音符贴段边缘
    pub fit: bool,
    /// 每个 key 行各跑一遍（仅 time）
    pub rows: bool,
    /// 跨空洞算一整段（仅 time）
    pub join: bool,
    /// 门长吸附到一半/两倍序列
    pub halves: bool,
    /// 仅 funnel：沿曲线
    pub follow_curve: bool,
}

impl Default for GateRangeParams {
    fn default() -> Self {
        Self {
            to: 0.25,
            graph: vec![[0.0, 0.0], [1.0, 1.0]],
            dir: RangeDir::Time,
            fit: false,
            rows: false,
            join: false,
            halves: false,
            follow_curve: false,
        }
    }
}

impl GateRangeParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝）
    pub fn validate(&self) -> Result<()> {
        if !self.to.is_finite() || !(0.0..=10_000.0).contains(&self.to) || self.to == 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "GateRange 的 to 必须在 (0, 10000]（实际 {}）",
                self.to
            )));
        }
        if self.graph.len() < 2 || self.graph.len() > 256 {
            return Err(CoreError::InvalidArgument(format!(
                "GateRange 的 graph 必须有 2..=256 个点（实际 {}）",
                self.graph.len()
            )));
        }
        let mut previous = f32::NEG_INFINITY;
        for (index, [u, y]) in self.graph.iter().enumerate() {
            if !u.is_finite()
                || !y.is_finite()
                || !(0.0..=1.0).contains(u)
                || !(0.0..=1.0).contains(y)
            {
                return Err(CoreError::InvalidArgument(format!(
                    "GateRange 的 graph[{index}] 必须在 0..=1（实际 [{u}, {y}]）"
                )));
            }
            if *u < previous {
                return Err(CoreError::InvalidArgument(
                    "GateRange 的 graph.u 必须按非降序排列".into(),
                ));
            }
            previous = *u;
        }
        if self.graph.first().is_some_and(|p| p[0] != 0.0)
            || self.graph.last().is_some_and(|p| p[0] != 1.0)
        {
            return Err(CoreError::InvalidArgument(
                "GateRange 的 graph 首 u 必须 0、末 u 必须 1".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
