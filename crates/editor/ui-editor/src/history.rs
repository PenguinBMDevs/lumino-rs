//! History module for undo/redo functionality — 重新导出自 lumino-note-core
//!
//! 保持与原有 `crate::history::*` 路径完全兼容。

pub use lumino_note_core::{EditorSnapshot, History, HistoryEntry, MoveOp, OperationEntry};
