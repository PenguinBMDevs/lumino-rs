//! 修饰链步骤与形状对象本身
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

use serde::{Deserialize, Serialize};

use super::params::TumourParams;
use super::{SHAPE_KEY_MAX, ShapeGeometry, ShapeKind, ShapeStyle};
use crate::error::{CoreError, Result};

/// 形状修饰链中的一步
///
/// 语义照抄 Spiderweb `notes/fx.py`：`ops` 是**有序**步骤，每一步作用在上一步输出的音符上，
/// 可开关、可重复（同一算子可以出现多次）。
///
/// `Noop` 证明"链式重跑"这条管线跑得通（幂等、顺序保真）；
/// 真实算子（Tumour / Claw / Strum / Chop / Glue / Sustain / GateFlip / …）
/// 由 T1 逐个**追加到末尾**。追加变体对旧数据安全，不需要提升部件版本。
///
/// v2 起本枚举**不再 `Copy + Eq`**：`Tumours` 含 `f32` 与 `graphs: Vec`。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum ShapeOp {
    /// 占位步骤：不改变任何音符
    #[default]
    Noop,
    /// 沿途突起（Geometry 阶段；DRAW-003）
    Tumours(TumourParams),
}

/// 算子在链上的执行阶段
///
/// `Geometry` 在「几何 → 音符」之前执行（改路径）；
/// `Notes` 在音符生成之后执行（改音符）。DRAW-004..010 按此分阶段追加。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpStage {
    /// 生成音符之前，作用于几何/路径
    Geometry,
    /// 生成音符之后，作用于音符
    Notes,
}

impl ShapeOp {
    /// 该算子所在的阶段
    pub fn stage(&self) -> OpStage {
        match self {
            Self::Noop | Self::Tumours(_) => OpStage::Geometry,
        }
    }

    /// 校验算子参数（加载路径不得 panic）
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Noop => Ok(()),
            Self::Tumours(params) => params
                .validate()
                .map_err(|e| CoreError::InvalidArgument(format!("Tumours 参数非法：{e}"))),
        }
    }
}

/// 一个形状（文档对象）
///
/// `id` 在文档内唯一且**跨撤销/重做稳定**（撤销恢复的是同一个形状，而不是新形状）。
/// `track` 用 `u16` 与 `LmtrackData` 的音轨 ID 对齐；`usize` 不进落盘结构。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeModel {
    /// 文档内唯一 ID
    pub id: u64,
    /// 所属音轨 ID
    pub track: u16,
    /// 几何类型
    pub kind: ShapeKind,
    /// 几何
    pub geometry: ShapeGeometry,
    /// 样式
    pub style: ShapeStyle,
    /// 有序修饰链（S0 恒为占位步骤）
    pub ops: Vec<ShapeOp>,
    /// 绘制时每 key 几拍（beats per key）——视图相关参数，必须随形状存储
    pub k: f32,
    /// 形状名（Shapes 列表显示用；可空）
    pub name: String,
    /// 是否隐藏（隐藏只影响显示，不影响生成音符）
    pub hidden: bool,
}

impl ShapeModel {
    /// 构造一个空几何形状
    pub fn new(id: u64, track: u16, kind: ShapeKind, k: f32) -> Self {
        Self {
            id,
            track,
            kind,
            geometry: ShapeGeometry::default(),
            style: ShapeStyle::default(),
            ops: Vec::new(),
            k,
            name: String::new(),
            hidden: false,
        }
    }

    /// 设置几何
    pub fn with_geometry(mut self, geometry: ShapeGeometry) -> Self {
        self.geometry = geometry;
        self
    }

    /// 设置样式
    pub fn with_style(mut self, style: ShapeStyle) -> Self {
        self.style = style;
        self
    }

    /// 追加一步修饰链
    pub fn push_op(&mut self, op: ShapeOp) {
        self.ops.push(op);
    }

    /// 校验形状数据是否可用于生成音符
    ///
    /// 返回 `Err(CoreError::InvalidArgument)` 而**不是 panic**：损坏或手改的工程文件
    /// 应当被拒绝并给出可读原因，不得让编辑器崩在加载路径上。
    pub fn validate(&self) -> Result<()> {
        if !self.k.is_finite() || self.k <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "形状 {} 的视图比例 k 非法（需有限且为正，实际 {}）",
                self.id, self.k
            )));
        }
        self.style.validate()?;
        for (index, op) in self.ops.iter().enumerate() {
            op.validate().map_err(|e| {
                CoreError::InvalidArgument(format!("形状 {} 的第 {index} 个算子非法：{e}", self.id))
            })?;
        }
        if self.geometry.is_empty() {
            return Err(CoreError::InvalidArgument(format!(
                "形状 {} 几何为空（既无点列也无外接框）",
                self.id
            )));
        }
        if let Some(rect) = &self.geometry.rect {
            if !rect.is_finite() {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的外接框含非有限值",
                    self.id
                )));
            }
            if !rect.is_normalized() {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的外接框未规范化（要求 lo <= hi）",
                    self.id
                )));
            }
            if rect.key_lo < 0.0 || rect.key_hi > f32::from(SHAPE_KEY_MAX) {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的外接框音高越界（允许 0..={}，实际 {}..={}）",
                    self.id, SHAPE_KEY_MAX, rect.key_lo, rect.key_hi
                )));
            }
        }
        for (index, anchor) in self.geometry.anchors.iter().enumerate() {
            if !anchor.is_finite() {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的第 {} 个锚点含非有限值",
                    self.id, index
                )));
            }
            let (tick, key) = anchor.pos;
            if tick < 0.0 {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的第 {} 个锚点 tick 为负（实际 {}）",
                    self.id, index, tick
                )));
            }
            if key < 0.0 || key > f32::from(SHAPE_KEY_MAX) {
                return Err(CoreError::InvalidArgument(format!(
                    "形状 {} 的第 {} 个锚点音高越界（允许 0..={}，实际 {}）",
                    self.id, index, SHAPE_KEY_MAX, key
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
