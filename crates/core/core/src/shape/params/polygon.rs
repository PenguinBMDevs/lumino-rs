//! Polygon / Star / Cross 参数（DRAW-015）
//!
//! 对应 Spiderweb `notes/polygon.py`：多边形是**参数化几何来源**，放置后仍可改
//! 点数 / 形态 / 内点比例 / 跳角。参数放 `ShapeOp::Polygon`（Geometry 阶段），
//! 因此不需要提升 `.lmshapes` 版本。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 多边形形态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PolyStyle {
    /// 正多边形（底边水平）
    #[default]
    Polygon,
    /// 星形（顶点朝上，外/内点交替）
    Star,
    /// 跳角星（每 skip 个角相连；可能多环）
    Cross,
}

/// 边上的图案引用（DRAW-016 的 Pattern 参数子集）
///
/// 本卡先落字段与校验，实际“每条边铺公式”留到 DRAW-016 接线。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatternRef {
    /// y = f(x)，x: 0→1 一圈
    pub formula: String,
    /// 公式变量
    pub vars: BTreeMap<String, f64>,
    /// 重复圈数
    pub loops: f32,
    /// beats per key
    pub k: f32,
    /// 另一侧
    pub mirror: bool,
    /// 侧向缩放
    pub scale: f32,
}

impl PatternRef {
    /// 参数是否合法
    pub fn validate(&self) -> Result<()> {
        if !self.loops.is_finite() || self.loops <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Polygon 边图案 loops 必须为正（实际 {}）",
                self.loops
            )));
        }
        if !self.k.is_finite() || self.k <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Polygon 边图案 k 必须为正（实际 {}）",
                self.k
            )));
        }
        if !self.scale.is_finite() {
            return Err(CoreError::InvalidArgument(
                "Polygon 边图案 scale 非法".into(),
            ));
        }
        Ok(())
    }
}

/// Polygon 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolygonParams {
    /// 角数（3..=200）
    pub points: u16,
    /// 形态
    pub style: PolyStyle,
    /// star 内点半径百分比（0..=1000，100 = 与外点同半径）
    pub inner: f32,
    /// cross：每 skip 个角相连（≥1）
    pub skip: u16,
    /// 每条边变成公式图案（DRAW-016；本卡只落字段）
    pub shape: Option<PatternRef>,
}

impl Default for PolygonParams {
    fn default() -> Self {
        Self {
            points: 5,
            style: PolyStyle::Polygon,
            inner: 50.0,
            skip: 2,
            shape: None,
        }
    }
}

impl PolygonParams {
    /// 校验参数（不 panic；损坏工程文件应被拒绝）
    pub fn validate(&self) -> Result<()> {
        if !(3..=200).contains(&self.points) {
            return Err(CoreError::InvalidArgument(format!(
                "Polygon 的 points 必须在 3..=200（实际 {}）",
                self.points
            )));
        }
        if !self.inner.is_finite() || !(0.0..=1000.0).contains(&self.inner) {
            return Err(CoreError::InvalidArgument(format!(
                "Polygon 的 inner 必须在 0..=1000（实际 {}）",
                self.inner
            )));
        }
        if self.skip == 0 {
            return Err(CoreError::InvalidArgument("Polygon 的 skip 必须 ≥1".into()));
        }
        if let Some(shape) = &self.shape {
            shape
                .validate()
                .map_err(|e| CoreError::InvalidArgument(format!("Polygon 边图案非法：{e}")))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
