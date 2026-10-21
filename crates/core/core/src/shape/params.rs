//! 形状算子参数
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

pub mod chop;
pub mod claw;
pub mod cycle;
pub mod funnel;
pub mod gate_range;
pub mod glue;
pub mod mask;
pub mod pattern;
pub mod polygon;
pub mod strum;
pub mod sustain;
pub mod tumours;

pub use chop::{ChopParams, ChopPiece};
pub use claw::{ClawMode, ClawParams, ClawPeriod, ClawTrash};
pub use cycle::{CycleBy, CycleParams, RowRestart};
pub use funnel::{CurveRef, FunnelParams, FunnelStart};
pub use gate_range::{GateRangeParams, RangeDir};
pub use glue::GlueParams;
pub use mask::{MaskMode, MaskParams, MaskShow};
pub use pattern::{LoopCurve, PatternParams, Sym};
pub use polygon::{PatternRef, PolyStyle, PolygonParams};
pub use strum::StrumParams;
pub use sustain::{StopsParams, SustainParams};
pub use tumours::{TumourGraphs, TumourParams, TumourShape, TumourSide, TumourWrap, factor_at};
