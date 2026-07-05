//! 编辑操作 — 剪切、复制、粘贴、全选、撤销、重做
//!
//! 注意：这些方法的实际实现分布在以下文件中：
//! - `cut_selected_notes()` / `copy_selected_notes()` / `paste_notes_from_clipboard()` → clipboard.rs
//! - `select_all_notes()` → note_ops.rs
//! - `undo()` / `redo()` → editor.rs（父模块）
//!
//! 此模块为占位模块，dispatch 入口在 `handle_action()` 中保持。

use crate::editor::Editor;
