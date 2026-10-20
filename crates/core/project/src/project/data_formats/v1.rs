//! `.lmshapes` v1 落盘结构（只解码）
//!
//! 字段顺序与 v1 当时的 `ShapeModel / ShapeGeometry / ShapeStyle` 逐一对齐；
//! 刻意不复用现行类型，避免以后再次演进时把两版结构缠在一起（DRAW-040 / D5）。
//!
//! **不得在新代码路径复用这些类型**：它们只服务于「老工程仍能打开」。

use lumino_core::shape::{
    ShapeAnchor, ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, ShapeRect, ShapeStyle,
    SpamAlign, SpamEnds,
};

use super::LmshapesData;

/// v1 `.lmshapes` 载荷（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct LmshapesDataV1 {
    shapes: Vec<ShapeModelV1>,
}

/// v1 形状对象（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeModelV1 {
    id: u64,
    track: u16,
    kind: ShapeKind,
    geometry: ShapeGeometryV1,
    style: ShapeStyleV1,
    ops: Vec<ShapeOp>,
    k: f32,
    name: String,
    hidden: bool,
}

/// v1 几何（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeGeometryV1 {
    anchors: Vec<ShapeAnchor>,
    rect: Option<ShapeRect>,
    closed: bool,
}

/// v1 样式（只解码）
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
struct ShapeStyleV1 {
    fill: ShapeFill,
    gate_beats: f32,
    stroke_width: u8,
    velocity: u8,
    colour_slot: u8,
}

impl From<LmshapesDataV1> for LmshapesData {
    fn from(v1: LmshapesDataV1) -> Self {
        Self {
            shapes: v1.shapes.into_iter().map(ShapeModel::from).collect(),
        }
    }
}

impl From<ShapeModelV1> for ShapeModel {
    fn from(v1: ShapeModelV1) -> Self {
        Self {
            id: v1.id,
            track: v1.track,
            kind: v1.kind,
            geometry: v1.geometry.into(),
            style: v1.style.into(),
            ops: v1.ops,
            k: v1.k,
            name: v1.name,
            hidden: v1.hidden,
        }
    }
}

impl From<ShapeGeometryV1> for ShapeGeometry {
    fn from(v1: ShapeGeometryV1) -> Self {
        Self {
            anchors: v1.anchors,
            rect: v1.rect,
            closed: v1.closed,
        }
    }
}

impl From<ShapeStyleV1> for ShapeStyle {
    fn from(v1: ShapeStyleV1) -> Self {
        Self {
            fill: v1.fill,
            gate_beats: v1.gate_beats,
            stroke_width: v1.stroke_width,
            velocity: v1.velocity,
            colour_slot: v1.colour_slot,
            // 每个 v2 新字段显式取默认值——禁止 `..Default::default()`：
            // 将来默认值变更时必须在这里被看见，而不是被静默吞掉。
            align: SpamAlign::Auto,
            ends: SpamEnds::Round,
            union: false,
            apart: false,
            borders: false,
            areas: Vec::new(),
        }
    }
}
