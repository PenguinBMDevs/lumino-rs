//! Formula / Pattern 参数（DRAW-016）
//!
//! 对应 Spiderweb `notes/pattern.py`：一条公式沿曲线铺开，曲线本身保留为 origin path。
//! 参数放 `ShapeOp::Pattern`（Geometry 阶段），追加变体不提升 `.lmshapes` 版本。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::expr::compile;

const MAX_LOOPS: f32 = 50_000.0;

/// 半对称方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sym {
    /// 后半段镜像前半段
    Mirror,
    /// 后半段转半圈
    Turn,
}

/// 手改的一圈（along 0..1, sideways keys）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoopCurve {
    /// 控制点 / 采样点（至少 2 个）
    pub pts: Vec<[f32; 2]>,
    /// 尖角索引（保留字段）
    pub sharp: Vec<u16>,
}

/// Pattern 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternParams {
    /// 预设 id（空 = 手输）
    pub preset: String,
    /// y = f(x)，x: 0→1 一圈，y = 侧向 key 数
    pub formula: String,
    /// 其它变量
    pub vars: BTreeMap<String, f64>,
    /// 沿曲线前后移动的公式（圈为单位）
    pub along: Option<String>,
    /// 重复圈数
    pub loops: f32,
    /// 多段曲线上每段各跑全部圈数
    pub each: bool,
    /// beats per key（与 Arc 同口径）
    pub k: f32,
    /// 另一侧
    pub mirror: bool,
    /// 侧向缩放
    pub scale: f32,
    /// 半对称
    pub sym: Option<Sym>,
    /// 手改环；优先于 formula
    pub loop_curve: Option<LoopCurve>,
}

impl Default for PatternParams {
    fn default() -> Self {
        Self {
            preset: String::new(),
            formula: "height * sin(x * 2 * pi)".into(),
            vars: BTreeMap::from([("height".to_string(), 4.0)]),
            along: None,
            loops: 4.0,
            each: false,
            k: 1.0,
            mirror: false,
            scale: 1.0,
            sym: None,
            loop_curve: None,
        }
    }
}

impl PatternParams {
    /// 校验参数（公式在此编译一次，坏公式在加载时就被拒绝）
    pub fn validate(&self) -> Result<()> {
        if !self.loops.is_finite() || self.loops <= 0.0 || self.loops > MAX_LOOPS {
            return Err(CoreError::InvalidArgument(format!(
                "Pattern 的 loops 必须在 (0, {MAX_LOOPS}]（实际 {}）",
                self.loops
            )));
        }
        if !self.k.is_finite() || self.k <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Pattern 的 k 必须为正（实际 {}）",
                self.k
            )));
        }
        if !self.scale.is_finite() || self.scale == 0.0 {
            return Err(CoreError::InvalidArgument(
                "Pattern 的 scale 必须非零且有限".into(),
            ));
        }
        for (name, value) in &self.vars {
            if name.trim().is_empty() || !value.is_finite() {
                return Err(CoreError::InvalidArgument(format!(
                    "Pattern 变量 {name:?} 非法（值 {value}）"
                )));
            }
        }
        if let Some(loop_curve) = &self.loop_curve {
            if loop_curve.pts.len() < 2 {
                return Err(CoreError::InvalidArgument(
                    "Pattern 手改环至少需要 2 个点".into(),
                ));
            }
            if loop_curve
                .pts
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(CoreError::InvalidArgument(
                    "Pattern 手改环含非有限值".into(),
                ));
            }
        } else if self.formula.trim().is_empty() {
            return Err(CoreError::InvalidArgument(
                "Pattern 公式为空且无手改环".into(),
            ));
        }
        let vars = self.vars.keys().map(String::as_str).collect::<Vec<_>>();
        if !self.formula.trim().is_empty() {
            compile(&self.formula, &vars)
                .map_err(|e| CoreError::InvalidArgument(format!("Pattern 公式非法：{e}")))?;
        }
        if let Some(along) = &self.along
            && !along.trim().is_empty()
        {
            compile(along, &vars)
                .map_err(|e| CoreError::InvalidArgument(format!("Pattern along 公式非法：{e}")))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
