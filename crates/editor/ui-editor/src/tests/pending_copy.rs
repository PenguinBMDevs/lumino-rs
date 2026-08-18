//! Ctrl+拖动复制音符测试：pending_copy_drag_state + 延迟提交
//!
//! 覆盖：
//! - `handle_pointer_pressed` Ctrl + 选择框内部 → `DraggingSelectionCopy`
//! - `handle_released` 松手后保存到 `pending_copy_drag_state`（不立即写入）
//! - `commit_pending_copy`：batch_insert 写入内存层并选中新副本
//! - 原始音符保持不变（复制不移动）
//! - `is_editing` / `has_pending_drag` / `commit_current_edit` 状态判定
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。
//!
//! 2026-08 拆分（原文件 1135 行 > 400 行阈值，按主题拆分子模块）：
//! - `commit`：commit_pending_copy 行为
//! - `release`：handle_released 松手行为（松手即提交）
//! - `pressed`：handle_pointer_pressed Ctrl+拖动 与状态判定/自动提交
//! - `mixed`：移动 + 复制并存（提交顺序正确性）
//! - `selection_box`：复制后只保留最新件（副本）框选
//! - `continuous`：连续复制与复制后移动（无 Ctrl 拖副本框）
//! - `copy_flow`：ghost 增量与完整交互序列回归
//! - `vertical`：上下拖动复制 BUG 复现

mod commit;
mod continuous;
mod copy_flow;
mod mixed;
mod pressed;
mod release;
mod selection_box;
mod vertical;

use crate::EditState;
use crate::Editor;
use lumino_editor_state::DragState;

/// 模拟 released.rs 中复制拖动松手：直接进入 DraggingSelectionCopy 再 handle_released
fn release_copy_drag(editor: &mut Editor, drag_state: DragState) {
    editor.editor_state.interaction.edit_state = EditState::DraggingSelectionCopy {
        drag_state: drag_state.clone(),
    };
    editor.handle_released();
}

// ===== 初始状态判定 =====

#[test]
fn test_pending_copy_state_initial_is_none() {
    let editor = Editor::new();
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(!editor.has_pending_drag());
    assert!(!editor.is_editing());
}
