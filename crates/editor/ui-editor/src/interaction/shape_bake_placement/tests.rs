//! DRAW-049：Bake 放置确认态（A+B）测试。
//!
//! 拆分口径（单文件 ≤600 行）：
//! - `placement`：放置主流程（✓ / × / 撤销 / 空白拖框 / 复位）
//! - `track`：归属轨（阶段 2 阻塞项：切轨收口 + 按归属轨提交）
//! - `handles`：9 锚点（4 角 + 4 边 + 中心）拖动
//! - `actions`：动作路由 / 多形状回退 / 换工具 / Insert / i2m 回归

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{EditState, ShapeKind};

mod actions;
mod handles;
mod placement;
mod track;

/// 3 轨文档、当前轨 1。
///
/// 切轨用例需要一条可切换过去的真实音轨（track 2）：`switch_to_track` 只写
/// `current_track` 不校验轨数，但按键音符数 / 文档重建都要求该轨存在。
fn test_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 3, 1, &[]);
    editor.editor_state.view.snap_precision = 1.0;
    editor.editor_state.line_tool.set_min_note_division(None);
    editor
}

/// 画一个 4 tick × 4 key 的矩形，停在「待确认形状」阶段。
fn draw_rect(editor: &mut Editor) {
    editor.set_shape(ShapeKind::Rectangle);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
}

/// pending 单支模型的几何 bounds。
fn pending_bounds(editor: &Editor) -> (f32, f32, f32, f32) {
    editor.editor_state.pending_composed[0].results[0]
        .model
        .geometry
        .bounds()
        .expect("pending 模型应有几何")
}

/// 构造一次框选（B 阶段拉框 / 点击共用）。
fn marquee_edit_state(
    start_tick: f32,
    current_tick: f32,
    start_key: u16,
    current_key: u16,
) -> EditState {
    EditState::Selecting {
        start_tick,
        current_tick,
        start_key,
        current_key,
        start_y: 0.0,
        current_y: 0.0,
    }
}

/// 指定音轨的音符数（不切轨，直接读文档）。
fn track_note_count(editor: &Editor, track: u16) -> usize {
    editor
        .editor_state
        .data
        .document
        .as_ref()
        .map(|doc| doc.track_note_count(track) as usize)
        .unwrap_or(0)
}

/// 当前文档历史（undo 栈）长度。
fn undo_len(editor: &Editor) -> usize {
    editor.editor_state.data.history.undo_len()
}

/// pending 预览音符快照 `(tick, key, length)`，供「拖动/复位后音符必须变化/还原」断言。
fn preview_note_snapshot(editor: &Editor) -> Vec<(f32, u16, f32)> {
    editor.editor_state.pending_composed[0].results[0]
        .notes
        .iter()
        .map(|note| (note.tick, note.key, note.length))
        .collect()
}
