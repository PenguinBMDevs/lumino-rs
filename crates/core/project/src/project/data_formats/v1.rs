//! `.lmshapes` v1 落盘结构（只解码）
//!
//! 字段顺序与 v1 当时的 `ShapeModel / ShapeGeometry / ShapeStyle` 逐一对齐；
//! 刻意不复用现行类型，避免以后再次演进时把两版结构缠在一起（DRAW-040 / D5）。
//!
//! v3 给 `ShapeAnchor` 追加 `bend` 后，v1 的顶层锚点与 `ShapeOp::Live`
//! 内嵌锚点都必须用 `ShapeAnchorV2`（v1 锚点布局与 v2 相同）解码。
//!
//! **不得在新代码路径复用这些类型**：它们只服务于「老工程仍能打开」。

use lumino_core::shape::{
    BetweenParams, ChopParams, ClawParams, CopyPathParams, CycleParams, FunnelParams,
    GateRangeParams, GlueParams, JoinedParams, LiveShapeParams, MaskParams, MergeParams,
    NestedParams, PatternParams, PolygonParams, ShapeAnchor, ShapeFill, ShapeGeometry, ShapeKind,
    ShapeModel, ShapeOp, ShapeRect, ShapeStyle, SpamAlign, SpamEnds, StraightenParams, StrumParams,
    SustainParams, SymParams, TextOp, TilesParams, TumourParams, VelocityEnv,
};

use super::LmshapesData;
use super::v2::ShapeAnchorV2;

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
    ops: Vec<ShapeOpV1>,
    k: f32,
    name: String,
    hidden: bool,
}

/// v1 几何（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeGeometryV1 {
    anchors: Vec<ShapeAnchorV2>,
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

/// v1 修饰链（只解码；仅 `Live` 的嵌套锚点需要专门结构）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum ShapeOpV1 {
    Noop,
    Tumours(TumourParams),
    Glue(GlueParams),
    Claw(ClawParams),
    Strum(StrumParams),
    Chop(ChopParams),
    GateFlip,
    Sustain(SustainParams),
    Mask(MaskParams),
    Cycle(CycleParams),
    GateRange(GateRangeParams),
    Polygon(PolygonParams),
    Pattern(PatternParams),
    Funnel(FunnelParams),
    Nested(NestedParams),
    Tiles(TilesParams),
    Live(LiveShapeParamsV1),
    Joined(JoinedParams),
    Merge(MergeParams),
    Symmetry(SymParams),
    Straighten(StraightenParams),
    Between(BetweenParams),
    CopyAlong(CopyPathParams),
    VelocityEnv(VelocityEnv),
    Text(TextOp),
}

/// v1 Live 参数（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LiveShapeParamsV1 {
    strokes: Vec<StrokeV1>,
    src: Vec<u32>,
    from: Vec<ShapeModelV1>,
}

/// v1 Live 段（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StrokeV1 {
    kind: lumino_core::shape::StrokeKind,
    anchors: Vec<ShapeAnchorV2>,
    rect: Option<ShapeRect>,
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
            ops: v1.ops.into_iter().map(ShapeOp::from).collect(),
            k: v1.k,
            name: v1.name,
            hidden: v1.hidden,
        }
    }
}

impl From<ShapeGeometryV1> for ShapeGeometry {
    fn from(v1: ShapeGeometryV1) -> Self {
        Self {
            anchors: v1.anchors.into_iter().map(ShapeAnchor::from).collect(),
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
            // 每个 v2/v3 新字段显式取默认值——禁止 `..Default::default()`：
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

impl From<ShapeOpV1> for ShapeOp {
    fn from(v1: ShapeOpV1) -> Self {
        match v1 {
            ShapeOpV1::Noop => Self::Noop,
            ShapeOpV1::Tumours(params) => Self::Tumours(params),
            ShapeOpV1::Glue(params) => Self::Glue(params),
            ShapeOpV1::Claw(params) => Self::Claw(params),
            ShapeOpV1::Strum(params) => Self::Strum(params),
            ShapeOpV1::Chop(params) => Self::Chop(params),
            ShapeOpV1::GateFlip => Self::GateFlip,
            ShapeOpV1::Sustain(params) => Self::Sustain(params),
            ShapeOpV1::Mask(params) => Self::Mask(params),
            ShapeOpV1::Cycle(params) => Self::Cycle(params),
            ShapeOpV1::GateRange(params) => Self::GateRange(params),
            ShapeOpV1::Polygon(params) => Self::Polygon(params),
            ShapeOpV1::Pattern(params) => Self::Pattern(params),
            ShapeOpV1::Funnel(params) => Self::Funnel(params),
            ShapeOpV1::Nested(params) => Self::Nested(params),
            ShapeOpV1::Tiles(params) => Self::Tiles(params),
            ShapeOpV1::Live(params) => Self::Live(params.into()),
            ShapeOpV1::Joined(params) => Self::Joined(params),
            ShapeOpV1::Merge(params) => Self::Merge(params),
            ShapeOpV1::Symmetry(params) => Self::Symmetry(params),
            ShapeOpV1::Straighten(params) => Self::Straighten(params),
            ShapeOpV1::Between(params) => Self::Between(params),
            ShapeOpV1::CopyAlong(params) => Self::CopyAlong(params),
            ShapeOpV1::VelocityEnv(env) => Self::VelocityEnv(env),
            ShapeOpV1::Text(op) => Self::Text(op),
        }
    }
}

impl From<LiveShapeParamsV1> for LiveShapeParams {
    fn from(v1: LiveShapeParamsV1) -> Self {
        Self {
            strokes: v1.strokes.into_iter().map(Into::into).collect(),
            src: v1.src,
            from: v1.from.into_iter().map(ShapeModel::from).collect(),
        }
    }
}

impl From<StrokeV1> for lumino_core::shape::Stroke {
    fn from(v1: StrokeV1) -> Self {
        Self {
            kind: v1.kind,
            anchors: v1.anchors.into_iter().map(ShapeAnchor::from).collect(),
            rect: v1.rect,
        }
    }
}
