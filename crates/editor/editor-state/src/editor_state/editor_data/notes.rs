//! 音符操作 —— CRUD、选择框、分割、合并、绘制
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `document`（MidiDocument），
//! 本模块所有操作直接读写当前音轨（`track_notes_mut` / 写访问器），
//! 不再维护 `notes` / `track_notes` 冗余副本。
//!
//! 子模块组织（保持本文件 < 400 行）：
//! - `drag`：拖动状态流式应用、绘制完成
//! - `editing`：分割、合并、连奏、删除增量事件合并
//! - `selection`：选择框计算、全选、删除

mod drag;
mod editing;
mod selection;

// 子模块与 `notes_tests.rs`（`use super::*`）均经 `super::` 引用以下符号
use super::EditorData;
use super::accessors;
use crate::DragState;
use lumino_note_core::note::Note;

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
