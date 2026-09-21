//! Undo/Redo 历史记录操作
//!
//! 2026-08 单一权威源改造：音符快照为 `Arc<Vec<NoteEvent>>`（从 document 轨道
//! 克隆，COW 共享）。恢复时经 `replace_track_notes` 写回 document。
//! `automation_lanes` 仍为 `Vec<Arc<AutomationLane>>`，编辑路径经 `Arc::make_mut` 写时复制。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::EditorData;
use super::{CollabCreateSyncEntry, CollabTransformSyncEntry};
use lumino_note_core::history::{CreateOp, EditorSnapshot, HistoryEntry, MoveOp, OpKind};

mod apply;
mod ops;
mod sync;
mod undo;

#[cfg(test)]
mod tests;
