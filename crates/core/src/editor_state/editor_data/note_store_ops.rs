//! NoteStore 集成操作：同步、批量移动、批量删除、批量插入
//!
//! 当音符数超过 `NOTE_STORE_THRESHOLD` 时自动启用 NoteStore 作为批量操作热路径。
//! `notes` (im::Vector) 仍为权威源，操作完成后通过 `sync_note_store()` 同步。
//!
//! ## 子模块
//! - `sync` — 同步启用/禁用 + BitVec→BitSet 转换
//! - `batch_move` — 批量移动（并行热路径 + 冷路径回退）
//! - `batch_edit` — 批量编辑属性（力度/长度/key/tick）
//! - `delete` — 批量删除
//! - `insert` — 批量/单个插入
//! - `access` — 状态查询与迭代访问器

mod sync;
mod batch_move;
mod batch_edit;
mod delete;
mod insert;
mod access;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
