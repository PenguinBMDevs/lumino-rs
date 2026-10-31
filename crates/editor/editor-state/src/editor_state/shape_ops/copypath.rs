//! Copy along a path（DRAW-027）
//!
//! 形状的 fx 页：沿 A→B→C… 复制音符。路径点相对组 box；副本角点落在路径上。

pub mod path;
pub mod place;

pub use path::{Dir, Placement, Pt, Run, corner_a, place_on_run, runs};
pub use place::{apply, copies, copies_of_points, copy_notes};
