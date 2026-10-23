//! Tiles 内部填充参数（DRAW-017）
//!
//! 形状保留自身 Fill / Spam，内部平铺另一图形（壁纸）；每块一个通道。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::shape::params::nested::DrawingRef;

/// 网格布局
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TileLayout {
    /// 正交网格
    #[default]
    Grid,
    /// 奇数行错半块
    Brick,
}

/// 镜像方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TileFlip {
    /// 不翻转
    #[default]
    None,
    /// 隔列左右
    H,
    /// 隔行上下
    V,
    /// 隔行 + 隔列
    Hv,
}

/// 块颜色依据
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TileBy {
    /// 对角线
    Diag,
    /// 列
    Cols,
    /// 行
    Rows,
    /// 棋盘
    Checker,
    /// 单一
    #[default]
    One,
}

/// Tiles 参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilesParams {
    /// 平铺的图形（本卡只支持 id=0 = 当前形状）
    pub of: DrawingRef,
    /// 单块高度（key 数）
    pub keys: f32,
    /// 单块长度（tick；与 gap 同单位）
    pub length: f32,
    /// 块间 key 间隙
    pub gap_keys: f32,
    /// 块间 tick 间隙
    pub gap: f32,
    /// 网格布局
    pub layout: TileLayout,
    /// 镜像
    pub flip: TileFlip,
    /// 块颜色依据
    pub by: TileBy,
    /// 颜色数（0 = 单一）
    pub colours: u8,
    /// 只保留未被轮廓切到的整块
    pub whole: bool,
    /// 平铺原点（0..1 的 box 坐标；None = 左下）
    pub at: Option<[f32; 2]>,
}

impl Default for TilesParams {
    fn default() -> Self {
        Self {
            of: DrawingRef::default(),
            keys: 4.0,
            length: 120.0,
            gap_keys: 0.0,
            gap: 0.0,
            layout: TileLayout::Grid,
            flip: TileFlip::None,
            by: TileBy::One,
            colours: 0,
            whole: false,
            at: None,
        }
    }
}

impl TilesParams {
    /// 校验参数
    pub fn validate(&self) -> Result<()> {
        if !self.keys.is_finite() || self.keys <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tiles 的 keys 必须为正（实际 {}）",
                self.keys
            )));
        }
        if !self.length.is_finite() || self.length <= 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tiles 的 length 必须为正（实际 {}）",
                self.length
            )));
        }
        if !self.gap_keys.is_finite() || self.gap_keys < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tiles 的 gap_keys 必须非负（实际 {}）",
                self.gap_keys
            )));
        }
        if !self.gap.is_finite() || self.gap < 0.0 {
            return Err(CoreError::InvalidArgument(format!(
                "Tiles 的 gap 必须非负（实际 {}）",
                self.gap
            )));
        }
        if self.colours > 16 {
            return Err(CoreError::InvalidArgument(format!(
                "Tiles 的 colours 必须在 0..=16（实际 {}）",
                self.colours
            )));
        }
        if let Some(at) = self.at
            && (!at[0].is_finite()
                || !at[1].is_finite()
                || !(0.0..=1.0).contains(&at[0])
                || !(0.0..=1.0).contains(&at[1]))
        {
            return Err(CoreError::InvalidArgument(
                "Tiles 的 at 必须是 0..=1 的两个有限数".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
