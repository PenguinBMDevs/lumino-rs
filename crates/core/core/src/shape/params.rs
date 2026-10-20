//! 形状算子参数
//!
//! 落盘纪律与模块划分见 [`super`] 的模块文档。

pub mod glue;
pub mod tumours;

pub use glue::GlueParams;
pub use tumours::{TumourGraphs, TumourParams, TumourShape, TumourSide, TumourWrap, factor_at};
