//! 形状修饰链执行器
//!
//! 两段式接口（DRAW-003 落点决策）：
//! - [`apply_geometry_ops`]：生成音符**之前**，作用于展平后的路径点列；
//! - [`apply_note_ops`]：生成音符**之后**，作用于音符（DRAW-004..010 陆续接入）。
//!
//! 这样 `editor-state` 不需要反向依赖 `ui-editor` 的 `Editor::shape_notes_of`，
//! 依赖方向保持 `ui-editor → editor-state → lumino-core`。

pub mod arc;
pub mod areas;
pub mod between;
pub mod bezier;
pub mod chain;
pub mod chop;
pub mod claw;
pub mod copypath;
pub mod cycle;
pub mod funnel;
pub mod gate_range;
pub mod gateflip;
pub mod glue;
pub mod joined;
pub mod live;
pub mod masks;
pub mod merge;
pub mod mould;
pub mod nested;
pub mod pattern;
pub mod polygon;
pub mod slice;
pub mod sliced;
pub mod smooth;
pub mod strum;
pub mod sustain;
pub mod tiles;
pub mod tumours;
pub mod velocity_tool;

#[cfg(test)]
mod tests;

pub use arc::{
    ARC_STEP, arc_k_or_one, arc_points, arc_points_scaled, arc_span, circle_through, rotate_k,
    stretch_k,
};
pub use areas::{AreaContext, row_pieces_coloured};
pub use between::{
    BetweenWarning, blend as between_blend, fingerprint as between_fingerprint,
    graph_at as between_graph_at, is_modified as between_is_modified, sync_groups,
};
pub use bezier::symmetry::{
    SymReject, symmetric, symmetric_shape, symmetric_shape_with_reason, symmetric_with_reason,
};
pub use bezier::{reverse_anchors, to_bezier};
pub use chain::{NoteOpContext, OpContext, apply_geometry_ops, apply_note_ops};
pub use chop::{ChopReport, apply as apply_chop};
pub use claw::apply as apply_claw;
pub use copypath::{
    apply as copy_along_apply, copies as copy_along_copies,
    copies_of_points as copy_along_copies_of_points, copy_notes as copy_along_notes,
};
pub use cycle::apply as apply_cycle;
pub use funnel::{
    SlantBox, box_outline as funnel_box_outline, funnel_rows, slant_boxes as funnel_slant_boxes,
};
pub use gate_range::{
    fit_squares, gate_at, keys_gate, normalize as normalize_gate_range, range_squares, steps_of,
};
pub use gateflip::{GateFlipReason, apply as apply_gate_flip, auto_gap};
pub use glue::apply as apply_glue;
pub use joined::{JoinedShape, Segment, join, split_here, split_separate};
pub use live::{LiveError, split_back, to_live};
pub use masks::{
    Area, MaskMember, MaskPreview, MaskedPiece, RowSpan, UnitArea, apply_visible, covered,
    member_of, preview, shown_parts, unit_area,
};
pub use merge::{MergeOutcome, comes_from_left, gate_merge};
pub use mould::{SHORT as MOULD_SHORT, mould_rows};
pub use nested::{ColouredSpan, nested_spans};
pub use pattern::{LOOP_SAMPLES, MAX_POINTS, PatternReport, formed_path, formed_path_with_report};
pub use polygon::{paths as polygon_paths, side_paths as polygon_side_paths, unit_loops};
pub use slice::{
    crossings, cut_loops, cut_open_vertical, inside as slice_inside, ring_of, split_notes_vertical,
};
pub use sliced::{CutKind, CutMark, CutRef, KnifeKind, outline_keys};
pub use smooth::{smooth_path, straighten_shape, tolerance as straighten_tolerance};
pub use strum::apply as apply_strum;
pub use sustain::{SustainReport, apply as apply_sustain};
pub use tiles::tiled_spans;
pub use tumours::{TumourReport, apply as apply_tumours};
pub use velocity_tool::{Drag, VelTool, apply_env_to_notes, apply_vel_tool};
