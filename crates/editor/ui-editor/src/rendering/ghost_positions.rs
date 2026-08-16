//! Ghost 增量位置构建：build_ghost_delta_positions + apply_ghost_delta
//!
//! 拆分原因：visible_notes.rs 接近 400 行限制，按职责拆分。
//! 拖动期间 document 未变（ghost 方案），UI 层通过
//! `build_ghost_delta_positions` 获取被拖动音符的 ghost 位置，
//! 生成 UpdateMany 增量发送替代每帧全量重建。

use super::Editor;
use super::ghost::{has_active_ghost_delta, is_note_ghosted};
use crate::EditState;

/// 从 EditState 提取当前 drag delta（Dragging 和 DraggingSelection 均有非零值）
///
/// `DraggingSelection` 也纳入提取，确保第二次拖动（已有 pending 时）
/// 当前 drag_state 的 delta 能应用到渲染位置，避免音符视觉位置不随鼠标移动。
#[inline]
pub(super) fn current_drag_delta(edit_state: &EditState) -> (i64, i16) {
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state } => {
            (drag_state.delta_tick, drag_state.delta_key)
        }
        _ => (0i64, 0i16),
    }
}

impl Editor {
    /// 是否存在活跃的 ghost 拖动（拖动中 / pending 异步提交 / 复制待提交）
    ///
    /// UI 层（渲染线程）判断是否走 ghost 增量路径。
    pub fn has_active_ghost_delta_state(&self) -> bool {
        has_active_ghost_delta(
            &self.pending_drag_state,
            &self.pending_copy_drag_state,
            &self.editor_state.interaction.edit_state,
        )
    }

    /// 构建 ghost 拖动增量的可见位置数据（仅「选中 ∩ 可见」的索引）
    ///
    /// 卷帘拖动增量（2026-08-05）：拖动期间 document 未变（ghost 方案），
    /// 只有被拖动的音符需要更新渲染位置。本方法返回
    /// `(可见列表位置, ghost 后位置 (tick, key, length))`（按可见位置升序），
    /// UI 层据此生成 UpdateMany 增量发送——替代每帧全量 collect+build+上传。
    ///
    /// 调用前提：`has_active_ghost_delta` 为 true 且 `visible_indices` 与
    /// 上次全量构建的可见索引一致（GPU 位置 = 列表下标）。
    ///
    /// **复制模式例外**：`DraggingSelectionCopy` / `pending_copy_drag_state`
    /// 会在可见列表中**追加副本实例**（原位置 + 偏移位置两条），破坏
    /// 「GPU 位置 = 列表下标」的增量前提。此状态下返回空列表，
    /// 调用方回退全量重建（正确性无损）。
    pub fn build_ghost_delta_positions(
        &self,
        visible_indices: &[usize],
    ) -> Vec<(usize, (f32, u16, f32))> {
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;
        let pending_copy = &self.pending_copy_drag_state;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let data = &self.editor_state.data;
        let (drag_dt, drag_dk) = current_drag_delta(edit_state);

        // 复制模式：副本实例使 GPU 布局 ≠ visible_indices 下标，禁用增量
        let any_copy =
            pending_copy.is_some() || matches!(edit_state, EditState::DraggingSelectionCopy { .. });
        if any_copy {
            return Vec::new();
        }

        let mut out = Vec::new();
        for (pos, &note_idx) in visible_indices.iter().enumerate() {
            if !is_note_ghosted(note_idx, pending, edit_state) {
                continue;
            }
            let Some((tick, key, length)) = data
                .get_note_view(note_idx)
                .map(|n| (n.tick, n.key, n.length))
            else {
                continue;
            };
            let (tick, key) =
                apply_ghost_delta(tick, key, drag_dt, drag_dk, pending, note_idx, max_key);
            out.push((pos, (tick, key, length)));
        }
        out
    }
}

/// 应用 ghost delta 到音符位置
///
/// 合并 drag_state delta 和 pending delta（如果音符在 pending 选中集合中）。
#[inline]
pub(super) fn apply_ghost_delta(
    tick: f32,
    key: u16,
    drag_dt: i64,
    drag_dk: i16,
    pending: &Option<lumino_editor_state::DragState>,
    i: usize,
    max_key: u16,
) -> (f32, u16) {
    let mut dt = drag_dt;
    let mut dk = drag_dk;
    if let Some(pending) = pending
        && i < pending.selected.len()
        && pending.selected[i]
    {
        dt = dt.saturating_add(pending.delta_tick);
        dk = dk.saturating_add(pending.delta_key);
    }
    (
        (tick + dt as f32).max(0.0),
        (key as i32 + dk as i32).clamp(0, max_key as i32) as u16,
    )
}
