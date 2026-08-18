//! 延迟提交方案测试：pending_drag_state + 累积 delta
//!
//! 覆盖：
//! - `DraggingSelection` 松手后保存到 `pending_drag_state`（不立即 apply）
//! - 累积 delta：多次拖动同一选区，delta 叠加
//! - `commit_pending_drag`：apply 到 document 并清空 pending
//! - `commit_current_edit` 在 pending 状态下提交
//! - `is_editing` / `has_pending_drag` 状态判定
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。
//!
//! 2026-08 拆分（原文件 532 行 > 400 行阈值，按主题拆分子模块）：
//! - `commit`：commit_pending_drag 行为与 pending 状态判定
//! - `accumulate`：累积 delta（多次拖动叠加）
//! - `deferred`：延迟提交流程、commit_current_edit 与点击空白处提交

mod accumulate;
mod commit;
mod deferred;

use crate::EditState;
use crate::Editor;
use lumino_editor_state::DragState;

/// 模拟 pressed.rs 中批量拖动入口：push_history（首次）+ 进入 DraggingSelection
///
/// **累积模式**：如果 pending_drag_state 已存在，不重复 push_history
/// （一次逻辑操作只产生一条 history 记录）
fn start_dragging_selection(
    editor: &mut Editor,
    indices: impl IntoIterator<Item = usize>,
    delta_tick: i64,
    delta_key: i16,
) {
    let note_count = editor.editor_state.data.current_track_note_count();
    // 累积模式：pending 存在时不重复 push_history
    if editor.pending_drag_state.is_none() {
        editor.push_history();
    }
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}

/// 提交 pending 并等待异步提交完成（测试用同步包装）
fn commit_pending_drag_and_drain(editor: &mut Editor) -> bool {
    let started = editor.commit_pending_drag();
    if started {
        editor.drain_async_commit();
    }
    started
}
