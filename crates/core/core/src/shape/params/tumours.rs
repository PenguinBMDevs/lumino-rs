//! Tumours（沿途突起）算子参数
//!
//! 语义对应 Spiderweb `notes/tumour.py`；本卡只落参数与校验，算法在
//! `lumino-editor-state` 的 `shape_ops/tumours.rs`。
//!
//! 单位固定：`size` = key，`length` / `dist` / `ease` = 拍，`rot` = 度，
//! `k` = 绘制时每 key 几拍（beats per key）。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

/// 凸起形态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TumourShape {
    /// 三角形
    #[default]
    Triangle,
    /// 方形（`slant` 只作用于它）
    Square,
    /// 半圆（屏幕上的圆度由 `k` 保证）
    Circle,
    /// 抛物线
    Parabola,
}

/// 凸起朝向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TumourSide {
    /// 交替（第 0 个向上，第 1 个向下……）
    #[default]
    Alt,
    /// 始终在路径左侧
    Left,
    /// 始终在路径右侧
    Right,
    /// 按 `seed` 伪随机
    Random,
}

/// 越过形状首尾时的处理
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TumourWrap {
    /// 只在 `start..end` 区间内布置（越界截断）
    #[default]
    Simple,
    /// 闭合路径首尾相接（当前实现按 Simple 截断，预留变体）
    Wrap,
}

/// 沿线图表：`(u, factor)`，`u` 为形状 0..1 位置，`factor` 为倍率（1 = 100%）
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TumourGraphs {
    /// 凸起高度倍率
    pub size: Vec<(f32, f32)>,
    /// 凸起长度倍率
    pub length: Vec<(f32, f32)>,
    /// 间距倍率（用于逐段改变疏密）
    pub dist: Vec<(f32, f32)>,
    /// 倾斜倍率
    pub rot: Vec<(f32, f32)>,
    /// 方顶收窄/加宽倍率
    pub slant: Vec<(f32, f32)>,
}

impl TumourGraphs {
    /// 校验每条曲线的点：`u ∈ 0..=1`、有限、按 `u` 非降序
    pub fn validate(&self) -> Result<()> {
        Self::validate_curve("size", &self.size)?;
        Self::validate_curve("length", &self.length)?;
        Self::validate_curve("dist", &self.dist)?;
        Self::validate_curve("rot", &self.rot)?;
        Self::validate_curve("slant", &self.slant)?;
        Ok(())
    }

    fn validate_curve(name: &str, points: &[(f32, f32)]) -> Result<()> {
        for (index, &(u, factor)) in points.iter().enumerate() {
            if !u.is_finite() || !factor.is_finite() {
                return Err(CoreError::InvalidArgument(format!(
                    "Tumours 的 graphs.{name} 第 {index} 个点含非有限值"
                )));
            }
            if !(0.0..=1.0).contains(&u) {
                return Err(CoreError::InvalidArgument(format!(
                    "Tumours 的 graphs.{name} 第 {index} 个点 u 越界（允许 0..=1，实际 {u}）"
                )));
            }
            if index > 0 && u < points[index - 1].0 {
                return Err(CoreError::InvalidArgument(format!(
                    "Tumours 的 graphs.{name} 必须按 u 非降序排列（第 {index} 个点 {u} < 前一点 {}）",
                    points[index - 1].0
                )));
            }
        }
        Ok(())
    }
}

/// 取曲线在 `u` 处的线性插值倍率；空曲线返回 1.0
pub fn factor_at(points: &[(f32, f32)], u: f32) -> f32 {
    let Some(&(first_u, first_factor)) = points.first() else {
        return 1.0;
    };
    let u = u.clamp(0.0, 1.0);
    if u <= first_u {
        return first_factor;
    }
    for pair in points.windows(2) {
        let (u0, f0) = pair[0];
        let (u1, f1) = pair[1];
        if u <= u1 {
            let span = u1 - u0;
            if span.abs() <= f32::EPSILON {
                return f1;
            }
            let t = (u - u0) / span;
            return f0 + (f1 - f0) * t;
        }
    }
    points.last().map(|&(_, factor)| factor).unwrap_or(1.0)
}

/// Tumours 参数（落盘字段，单位固定）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TumourParams {
    /// 是否启用（关闭时算法原样返回路径）
    pub on: bool,
    /// 凸起形态
    pub shape: TumourShape,
    /// 凸起高度（key，可负）
    pub size: f32,
    /// 沿路径长度（拍；0 = 尖刺）
    pub length: f32,
    /// 相邻凸起起点间距（拍，> 0）
    pub dist: f32,
    /// 法线方向
    pub side: TumourSide,
    /// 首尾越界处理
    pub wrap: TumourWrap,
    /// 只在该段布置（0..=1）
    pub start: f32,
    /// 只在该段布置（0..=1）
    pub end: f32,
    /// 两端渐入长度（拍）
    pub ease: f32,
    /// 绕两脚连线倾斜（度，-180..=180）
    pub rot: f32,
    /// 仅 Square：顶边收窄/加宽（-1..=1）
    pub slant: f32,
    /// 距离微调，使整数个凸起正好铺满
    pub fit: bool,
    /// Random 朝向的随机种子
    pub seed: u32,
    /// 镜像朝向
    pub mirror: bool,
    /// 视图比例：每 key 几拍（beats per key）
    pub k: f32,
    /// 沿线图表
    pub graphs: TumourGraphs,
}

impl Default for TumourParams {
    fn default() -> Self {
        Self {
            on: true,
            shape: TumourShape::Triangle,
            size: 4.0,
            length: 1.0,
            dist: 1.0,
            side: TumourSide::Alt,
            wrap: TumourWrap::Simple,
            start: 0.0,
            end: 1.0,
            ease: 0.0,
            rot: 0.0,
            slant: 0.0,
            fit: false,
            seed: 0,
            mirror: false,
            k: 0.25,
            graphs: TumourGraphs::default(),
        }
    }
}

impl TumourParams {
    /// 校验参数（不 panic；非法工程文件应被拒绝并给出可读原因）
    pub fn validate(&self) -> Result<()> {
        let finite = [
            self.size,
            self.length,
            self.dist,
            self.start,
            self.end,
            self.ease,
            self.rot,
            self.slant,
            self.k,
        ];
        if finite.iter().any(|value| !value.is_finite()) {
            return Err(CoreError::InvalidArgument("Tumours 参数含非有限值".into()));
        }
        if self.dist <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 dist 必须为正（实际 {}）",
                self.dist
            )));
        }
        if self.length < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 length 不能为负（实际 {}）",
                self.length
            )));
        }
        if self.ease < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 ease 不能为负（实际 {}）",
                self.ease
            )));
        }
        if !(0.0..=1.0).contains(&self.start) || !(0.0..=1.0).contains(&self.end) {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 start/end 必须在 0..=1（实际 {}..={}）",
                self.start, self.end
            )));
        }
        if !(-180.0..=180.0).contains(&self.rot) {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 rot 必须在 -180..=180（实际 {}）",
                self.rot
            )));
        }
        if !(-1.0..=1.0).contains(&self.slant) {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 slant 必须在 -1..=1（实际 {}）",
                self.slant
            )));
        }
        if self.k <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tumours 的 k 必须为正（实际 {}）",
                self.k
            )));
        }
        self.graphs.validate()
    }
}

#[cfg(test)]
mod tests;
