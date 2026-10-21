//! 形状修饰链执行器
//!
//! 两段式接口（DRAW-003 落点决策）：
//! - [`apply_geometry_ops`]：生成音符**之前**，作用于展平后的路径点列；
//! - [`apply_note_ops`]：生成音符**之后**，作用于音符（DRAW-004..010 陆续接入）。
//!
//! 这样 `editor-state` 不需要反向依赖 `ui-editor` 的 `Editor::shape_notes_of`，
//! 依赖方向保持 `ui-editor → editor-state → lumino-core`。

pub mod arc;
pub mod chain;
pub mod chop;
pub mod claw;
pub mod cycle;
pub mod gate_range;
pub mod gateflip;
pub mod glue;
pub mod masks;
pub mod polygon;
pub mod strum;
pub mod sustain;
pub mod tumours;

#[cfg(test)]
mod tests;

pub use arc::{
    ARC_STEP, arc_k_or_one, arc_points, arc_points_scaled, arc_span, circle_through, rotate_k,
    stretch_k,
};
pub use chain::{NoteOpContext, OpContext, apply_geometry_ops, apply_note_ops};
pub use chop::{ChopReport, apply as apply_chop};
pub use claw::apply as apply_claw;
pub use cycle::apply as apply_cycle;
pub use gate_range::{
    fit_squares, gate_at, keys_gate, normalize as normalize_gate_range, range_squares, steps_of,
};
pub use gateflip::{GateFlipReason, apply as apply_gate_flip, auto_gap};
pub use glue::apply as apply_glue;
pub use masks::{
    Area, MaskMember, MaskPreview, MaskedPiece, RowSpan, UnitArea, apply_visible, covered,
    member_of, preview, shown_parts, unit_area,
};
pub use polygon::{paths as polygon_paths, side_paths as polygon_side_paths, unit_loops};
pub use strum::apply as apply_strum;
pub use sustain::{SustainReport, apply as apply_sustain};
pub use tumours::{TumourReport, apply as apply_tumours};
