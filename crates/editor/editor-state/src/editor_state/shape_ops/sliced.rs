//! Slice 片的簿记（DRAW-025）
//!
//! 片记住原形状与当时的轮廓 keys；轮廓一变就退化为普通形状。
//! 完整的音符合并/分割在 UI 生成链路里完成，本模块只提供数据与失效判定。

use lumino_core::shape::ShapeModel;

/// 刀具类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnifeKind {
    /// 直线切
    Line,
    /// 曲线切（预留）
    Curve,
}

/// 切口类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutKind {
    /// Split here
    Split,
    /// Slice 工具
    Slice,
}

/// 切口标记
#[derive(Debug, Clone, PartialEq)]
pub struct CutMark {
    /// 标记 id
    pub id: u32,
    /// 锚点/行号
    pub at: usize,
    /// 切口在原路径上的比例
    pub u: f32,
    /// 切口类型
    pub kind: CutKind,
    /// 切口方向（切线方向）
    pub dir: (f32, f32),
}

/// 一片的簿记
#[derive(Debug, Clone, PartialEq)]
pub struct CutRef {
    /// 原形状（含当时位置）
    pub whole: ShapeModel,
    /// 本片当时的轮廓 keys
    pub was: Vec<u16>,
    /// 本片起始于原路径的哪段；None = 到末尾
    pub part: (f32, Option<f32>),
    /// 刀具
    pub knife: Option<KnifeKind>,
    /// 本片当时的力度
    pub vel: Option<Vec<u8>>,
    /// 切口标记
    pub marks: Vec<CutMark>,
}

impl CutRef {
    /// 由原形状构造一片；`was` 为当时的轮廓 keys
    pub fn new(whole: ShapeModel, part: (f32, Option<f32>)) -> Self {
        let was = outline_keys(&whole);
        Self {
            whole,
            was,
            part,
            knife: None,
            vel: None,
            marks: Vec::new(),
        }
    }

    /// 轮廓是否仍与切片时一致
    pub fn still_valid(&self, current: &ShapeModel) -> bool {
        self.was == outline_keys(current) && self.whole.kind == current.kind
    }

    /// 轮廓变化则清除切片身份；返回是否仍有效
    pub fn validate_against(&mut self, current: &ShapeModel) -> bool {
        if self.still_valid(current) {
            return true;
        }
        self.knife = None;
        self.vel = None;
        self.marks.clear();
        false
    }
}

/// 轮廓指纹：锚点 key 与框型边界；用于“改轮廓即失效”
pub fn outline_keys(shape: &ShapeModel) -> Vec<u16> {
    let mut keys = shape
        .geometry
        .anchors
        .iter()
        .map(|anchor| anchor.pos.1.round().max(0.0) as u16)
        .collect::<Vec<_>>();
    if let Some(rect) = shape.geometry.rect {
        keys.push(rect.key_lo.round().max(0.0) as u16);
        keys.push(rect.key_hi.round().max(0.0) as u16);
    }
    keys
}

#[cfg(test)]
mod tests;
