//! 形状修饰链执行器
//!
//! 两段式接口（DRAW-003 落点决策）：
//! - [`apply_geometry_ops`]：生成音符**之前**，作用于展平后的路径点列；
//! - [`apply_note_ops`]：生成音符**之后**，作用于音符（DRAW-004..010 陆续接入）。
//!
//! 这样 `editor-state` 不需要反向依赖 `ui-editor` 的 `Editor::shape_notes_of`，
//! 依赖方向保持 `ui-editor → editor-state → lumino-core`。

pub mod chain;
pub mod claw;
pub mod glue;
pub mod tumours;

#[cfg(test)]
mod tests;

pub use chain::{NoteOpContext, OpContext, apply_geometry_ops, apply_note_ops};
pub use claw::apply as apply_claw;
pub use glue::apply as apply_glue;
pub use tumours::{TumourReport, apply as apply_tumours};
