//! `.lmshapes` v2 落盘结构（只解码）
//!
//! v2 是 DRAW-040 引入 `ShapeStyle` 新字段后的结构；v3 只给
//! `ShapeAnchor` 追加 `bend`。这里刻意不复用现行类型：`ShapeOp::Live`
//! 内嵌 `Stroke.anchors` 与 `from: Vec<ShapeModel>`，直接复用现行
//! `ShapeOp` 会让旧 v2 的 Live 解码错位。
//!
//! **不得在新代码路径复用这些类型**：它们只服务于「v2 老工程仍能打开」。

use lumino_core::shape::{
    AreaSpot, BetweenParams, ChopParams, ClawParams, CopyPathParams, CycleParams, FunnelParams,
    GateRangeParams, GlueParams, JoinedParams, LiveShapeParams, MaskParams, MergeParams,
    NestedParams, PatternParams, PolygonParams, ShapeAnchor, ShapeFill, ShapeGeometry, ShapeKind,
    ShapeModel, ShapeOp, ShapeRect, ShapeStyle, SpamAlign, SpamEnds, StraightenParams, StrumParams,
    SustainParams, SymParams, TextOp, TilesParams, TumourParams, VelocityEnv,
};

use super::LmshapesData;

/// v2 `.lmshapes` 载荷（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct LmshapesDataV2 {
    shapes: Vec<ShapeModelV2>,
}

/// v2 形状对象（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeModelV2 {
    id: u64,
    track: u16,
    kind: ShapeKind,
    geometry: ShapeGeometryV2,
    style: ShapeStyleV2,
    ops: Vec<ShapeOpV2>,
    k: f32,
    name: String,
    hidden: bool,
}

/// v2 几何（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeGeometryV2 {
    anchors: Vec<ShapeAnchorV2>,
    rect: Option<ShapeRect>,
    closed: bool,
}

/// v2 锚点（只解码；无 `bend`）
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(super) struct ShapeAnchorV2 {
    pos: (f32, f32),
    in_handle: (f32, f32),
    out_handle: (f32, f32),
    handles_auto: bool,
}

/// v2 样式（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ShapeStyleV2 {
    fill: ShapeFill,
    gate_beats: f32,
    stroke_width: u8,
    velocity: u8,
    colour_slot: u8,
    align: SpamAlign,
    ends: SpamEnds,
    union: bool,
    apart: bool,
    borders: bool,
    #[serde(default)]
    areas: Vec<AreaSpot>,
}

/// v2 修饰链（只解码；仅 `Live` 的嵌套锚点需要专门结构）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum ShapeOpV2 {
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
    Live(LiveShapeParamsV2),
    Joined(JoinedParams),
    Merge(MergeParams),
    Symmetry(SymParams),
    Straighten(StraightenParams),
    Between(BetweenParams),
    CopyAlong(CopyPathParams),
    VelocityEnv(VelocityEnv),
    Text(TextOp),
}

/// v2 Live 参数（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LiveShapeParamsV2 {
    strokes: Vec<StrokeV2>,
    src: Vec<u32>,
    from: Vec<ShapeModelV2>,
}

/// v2 Live 段（只解码）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StrokeV2 {
    kind: lumino_core::shape::StrokeKind,
    anchors: Vec<ShapeAnchorV2>,
    rect: Option<ShapeRect>,
}

impl From<LmshapesDataV2> for LmshapesData {
    fn from(v2: LmshapesDataV2) -> Self {
        Self {
            shapes: v2.shapes.into_iter().map(ShapeModel::from).collect(),
        }
    }
}

impl From<ShapeModelV2> for ShapeModel {
    fn from(v2: ShapeModelV2) -> Self {
        Self {
            id: v2.id,
            track: v2.track,
            kind: v2.kind,
            geometry: v2.geometry.into(),
            style: v2.style.into(),
            ops: v2.ops.into_iter().map(ShapeOp::from).collect(),
            k: v2.k,
            name: v2.name,
            hidden: v2.hidden,
        }
    }
}

impl From<ShapeGeometryV2> for ShapeGeometry {
    fn from(v2: ShapeGeometryV2) -> Self {
        Self {
            anchors: v2.anchors.into_iter().map(ShapeAnchor::from).collect(),
            rect: v2.rect,
            closed: v2.closed,
        }
    }
}

impl From<ShapeAnchorV2> for ShapeAnchor {
    fn from(v2: ShapeAnchorV2) -> Self {
        Self {
            pos: v2.pos,
            in_handle: v2.in_handle,
            out_handle: v2.out_handle,
            handles_auto: v2.handles_auto,
            // v2 没有弯折字段；升级默认平滑。
            bend: lumino_core::shape::AnchorBend::Smooth,
        }
    }
}

impl From<ShapeStyleV2> for ShapeStyle {
    fn from(v2: ShapeStyleV2) -> Self {
        Self {
            fill: v2.fill,
            gate_beats: v2.gate_beats,
            stroke_width: v2.stroke_width,
            velocity: v2.velocity,
            colour_slot: v2.colour_slot,
            align: v2.align,
            ends: v2.ends,
            union: v2.union,
            apart: v2.apart,
            borders: v2.borders,
            areas: v2.areas,
        }
    }
}

impl From<ShapeOpV2> for ShapeOp {
    fn from(v2: ShapeOpV2) -> Self {
        match v2 {
            ShapeOpV2::Noop => Self::Noop,
            ShapeOpV2::Tumours(params) => Self::Tumours(params),
            ShapeOpV2::Glue(params) => Self::Glue(params),
            ShapeOpV2::Claw(params) => Self::Claw(params),
            ShapeOpV2::Strum(params) => Self::Strum(params),
            ShapeOpV2::Chop(params) => Self::Chop(params),
            ShapeOpV2::GateFlip => Self::GateFlip,
            ShapeOpV2::Sustain(params) => Self::Sustain(params),
            ShapeOpV2::Mask(params) => Self::Mask(params),
            ShapeOpV2::Cycle(params) => Self::Cycle(params),
            ShapeOpV2::GateRange(params) => Self::GateRange(params),
            ShapeOpV2::Polygon(params) => Self::Polygon(params),
            ShapeOpV2::Pattern(params) => Self::Pattern(params),
            ShapeOpV2::Funnel(params) => Self::Funnel(params),
            ShapeOpV2::Nested(params) => Self::Nested(params),
            ShapeOpV2::Tiles(params) => Self::Tiles(params),
            ShapeOpV2::Live(params) => Self::Live(params.into()),
            ShapeOpV2::Joined(params) => Self::Joined(params),
            ShapeOpV2::Merge(params) => Self::Merge(params),
            ShapeOpV2::Symmetry(params) => Self::Symmetry(params),
            ShapeOpV2::Straighten(params) => Self::Straighten(params),
            ShapeOpV2::Between(params) => Self::Between(params),
            ShapeOpV2::CopyAlong(params) => Self::CopyAlong(params),
            ShapeOpV2::VelocityEnv(env) => Self::VelocityEnv(env),
            ShapeOpV2::Text(op) => Self::Text(op),
        }
    }
}

impl From<LiveShapeParamsV2> for LiveShapeParams {
    fn from(v2: LiveShapeParamsV2) -> Self {
        Self {
            strokes: v2.strokes.into_iter().map(Into::into).collect(),
            src: v2.src,
            from: v2.from.into_iter().map(ShapeModel::from).collect(),
        }
    }
}

impl From<StrokeV2> for lumino_core::shape::Stroke {
    fn from(v2: StrokeV2) -> Self {
        Self {
            kind: v2.kind,
            anchors: v2.anchors.into_iter().map(ShapeAnchor::from).collect(),
            rect: v2.rect,
        }
    }
}
