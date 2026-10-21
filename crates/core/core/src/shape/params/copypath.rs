//! Copy along a path 参数（DRAW-027）
//!
//! 形状作为 fx 页沿 A→B→C… 反复复制；路径点相对组 box 存储，
//! 副本的角点落在路径上。走 `ShapeOp::CopyAlong`，不提升 `.lmshapes` 版本。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::PatternParams;

/// 路径类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PathKind {
    /// 直线段
    #[default]
    Straight,
    /// 曲线
    Curve,
    /// 屏幕上的圆弧
    Arc,
}

/// 副本数量规则
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CopyMode {
    /// 每 N 拍一个
    Every {
        /// 拍数
        beats: f32,
    },
    /// 沿路径共 N 个
    Count {
        /// 副本数
        n: u32,
    },
}

impl Default for CopyMode {
    fn default() -> Self {
        Self::Count { n: 1 }
    }
}

/// 速度分布
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Speed {
    /// 均匀
    #[default]
    Even,
    /// 由疏到密
    Up,
    /// 由密到疏
    Down,
    /// 自定义曲线
    Custom,
}

/// 副本颜色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColourMode {
    /// 保持原通道
    #[default]
    Keep,
    /// 每次偏移
    Shift {
        /// -15..=15
        step: i8,
    },
    /// 轮转
    Turn,
    /// 全部设为同一通道
    One {
        /// 0..=16
        colour: u8,
    },
}

/// 翻转轴
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlipWay {
    /// 上下
    #[default]
    Keys,
    /// 左右
    Time,
    /// 双向
    Both,
}

/// 翻转方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlipMode {
    /// 不翻
    #[default]
    None,
    /// 每个副本按路径方向翻
    Each {
        /// 翻转轴
        way: FlipWay,
    },
    /// 路径峰/谷后镜像
    Mirror,
}

/// Copy along a path 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopyPathParams {
    /// B, C, …（相对组 box；空 = B 未放置、本页不生效）
    pub pts: Vec<[f32; 2]>,
    /// 每段中点（None = 直线段）
    pub mids: Vec<Option<[f32; 2]>>,
    /// 路径类型
    pub kind: PathKind,
    /// 沿整条路径的公式
    pub pattern: Option<PatternParams>,
    /// 数量规则
    pub mode: CopyMode,
    /// true = 副本角落在点上；false = 沿整条路径均分
    pub anchor: bool,
    /// 转弯处副本继续越过
    pub half: bool,
    /// 制此页时的 beats per key
    pub k: f32,
    /// 速度分布
    pub speed: Speed,
    /// Custom speed 的曲线 `[[副本 0..1, 路径位置 0..1, bend]]`
    pub curve: Option<Vec<[f32; 3]>>,
    /// 每段重新开始 Speed
    pub restart: bool,
    /// 首末副本尺寸百分比
    pub size: [f32; 2],
    /// 颜色
    pub colours: ColourMode,
    /// 翻转
    pub flip: FlipMode,
    /// 组 box 的首起点与最低 key
    pub grp: Option<[f32; 2]>,
}

impl Default for CopyPathParams {
    fn default() -> Self {
        Self {
            pts: Vec::new(),
            mids: Vec::new(),
            kind: PathKind::Straight,
            pattern: None,
            mode: CopyMode::default(),
            anchor: false,
            half: false,
            k: 1.0,
            speed: Speed::Even,
            curve: None,
            restart: false,
            size: [100.0, 100.0],
            colours: ColourMode::Keep,
            flip: FlipMode::None,
            grp: None,
        }
    }
}

impl CopyPathParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        if !self.k.is_finite() || self.k <= 0.0 {
            return Err(CoreError::InvalidArgument("CopyAlong 的 k 必须为正".into()));
        }
        if self
            .size
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(CoreError::InvalidArgument(
                "CopyAlong 的 size 必须是非负有限值".into(),
            ));
        }
        if self
            .pts
            .iter()
            .flatten()
            .chain(self.mids.iter().flatten().flatten())
            .chain(self.grp.iter().flatten())
            .any(|value| !value.is_finite())
        {
            return Err(CoreError::InvalidArgument(
                "CopyAlong 的路径点必须有限".into(),
            ));
        }
        match self.mode {
            CopyMode::Every { beats } if !beats.is_finite() || beats <= 0.0 => {
                return Err(CoreError::InvalidArgument(
                    "CopyAlong 的 Every.beats 必须为正".into(),
                ));
            }
            CopyMode::Count { n: 0 } => {
                return Err(CoreError::InvalidArgument(
                    "CopyAlong 的 Count.n 必须大于 0".into(),
                ));
            }
            _ => {}
        }
        if let ColourMode::Shift { step } = self.colours
            && !(-15..=15).contains(&step)
        {
            return Err(CoreError::InvalidArgument(
                "CopyAlong 的 colours.step 必须在 -15..=15".into(),
            ));
        }
        if let ColourMode::One { colour } = self.colours
            && colour > 16
        {
            return Err(CoreError::InvalidArgument(
                "CopyAlong 的 colours.colour 不能超过 16".into(),
            ));
        }
        if let Some(pattern) = &self.pattern {
            pattern.validate().map_err(|e| {
                CoreError::InvalidArgument(format!("CopyAlong 的 pattern 非法：{e}"))
            })?;
        }
        if let Some(curve) = &self.curve
            && curve.iter().flatten().any(|value| !value.is_finite())
        {
            return Err(CoreError::InvalidArgument(
                "CopyAlong 的 speed 曲线含非有限值".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
