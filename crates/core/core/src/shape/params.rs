//! 形状算子参数
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

pub mod chop;
pub mod claw;
pub mod glue;
pub mod strum;
pub mod sustain;
pub mod tumours;

pub use chop::{ChopParams, ChopPiece};
pub use claw::{ClawMode, ClawParams, ClawPeriod, ClawTrash};
pub use glue::GlueParams;
pub use strum::StrumParams;
pub use sustain::{StopsParams, SustainParams};
pub use tumours::{TumourGraphs, TumourParams, TumourShape, TumourSide, TumourWrap, factor_at};
