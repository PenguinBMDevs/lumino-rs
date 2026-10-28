//! 形状算子参数
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

pub mod between;
pub mod chop;
pub mod claw;
pub mod copypath;
pub mod cycle;
pub mod drawing;
pub mod funnel;
pub mod gate_range;
pub mod glue;
pub mod joined;
pub mod live;
pub mod mask;
pub mod merge;
pub mod nested;
pub mod pattern;
pub mod polygon;
pub mod straighten;
pub mod strum;
pub mod sustain;
pub mod symmetry;
pub mod tiles;
pub mod tumours;

pub use between::{BetweenParams, BetweenRole, BetweenSet};
pub use chop::{ChopParams, ChopPiece};
pub use claw::{ClawMode, ClawParams, ClawPeriod, ClawTrash};
pub use copypath::{ColourMode, CopyMode, CopyPathParams, FlipMode, FlipWay, PathKind, Speed};
pub use cycle::{CycleBy, CycleParams, RowRestart};
pub use drawing::{DrawingParams, StrokeRole};
pub use funnel::{CurveRef, FunnelParams, FunnelStart};
pub use gate_range::{GateRangeParams, RangeDir};
pub use glue::GlueParams;
pub use joined::JoinedParams;
pub use live::{LiveShapeParams, LiveWarning, Stroke, StrokeKind};
pub use mask::{MaskMode, MaskParams, MaskShow};
pub use merge::MergeParams;
pub use nested::{DrawingRef, NestedParams, NestedWay};
pub use pattern::{LoopCurve, PatternParams, Sym};
pub use polygon::{PatternRef, PolyStyle, PolygonParams};
pub use straighten::StraightenParams;
pub use strum::StrumParams;
pub use sustain::{StopsParams, SustainParams};
pub use symmetry::{SymAxis, SymParams};
pub use tiles::{TileBy, TileFlip, TileLayout, TilesParams};
pub use tumours::{TumourGraphs, TumourParams, TumourShape, TumourSide, TumourWrap, factor_at};
