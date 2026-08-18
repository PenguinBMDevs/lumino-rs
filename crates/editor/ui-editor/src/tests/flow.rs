//! 端到端集成测试：ghost 拖动 → 提交 → 撤销 → 重做 完整流程
//!
//! 验证 DragState + Editor + History 三个模块的协作：
//! - 单音符拖动完整流程（push_history → 进入 Dragging → commit → undo → redo）
//! - 批量拖动完整流程
//! - 连续多次拖动的独立 undo 链
//! - 跨 group 逻辑撤销（直接调用 data.undo_logical）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。
//!
//! 2026-08 拆分（原文件 473 行 > 400 行阈值，按主题拆分子模块）：
//! - `single`：单音符拖动完整流程
//! - `batch`：批量拖动完整流程与连续多次拖动（独立 undo 链）
//! - `undo`：零 delta 拖动与跨 group 逻辑撤销

mod batch;
mod single;
mod undo;

use crate::EditState;
use crate::Editor;
use lumino_editor_state::DragState;

/// 模拟用户完整拖动一个音符的流程：
/// push_history（按下时） → 进入 Dragging（移动时） → commit（松手时）
fn setup_dragging(editor: &mut Editor, note_index: usize, delta_tick: i64, delta_key: i16) {
    let note_count = editor.editor_state.data.current_track_note_count();
    let original_tick = editor
        .editor_state
        .data
        .get_note_view(note_index)
        .expect("note 应存在")
        .tick as i64;
    let original_key = editor
        .editor_state
        .data
        .get_note_view(note_index)
        .expect("note 应存在")
        .key as i16;

    // 模拟 try_start_drag：进入 Dragging
    // 注意：单音符拖动现在走 MoveOp，finalize_dragging 会自己 push 操作日志，
    // 这里不再额外 push 快照，避免两次拖动之间出现冗余快照。
    let mut drag = DragState::from_single(note_index, note_count, original_tick, original_key);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index,
        drag_state: drag,
        last_played_key: original_key as u16,
    };
}

/// 模拟用户完整批量拖动流程
fn setup_dragging_selection(
    editor: &mut Editor,
    indices: impl IntoIterator<Item = usize>,
    delta_tick: i64,
    delta_key: i16,
) {
    let note_count = editor.editor_state.data.current_track_note_count();

    // 模拟 pressed.rs 中的批量拖动入口：push_history + 进入 DraggingSelection
    editor.push_history();
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}
