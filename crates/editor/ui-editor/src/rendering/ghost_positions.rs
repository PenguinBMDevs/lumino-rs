//! Ghost 增量位置构建：build_ghost_delta_positions + apply_ghost_delta
//!
//! 拆分原因：visible_notes.rs 接近 400 行限制，按职责拆分。
//! 拖动期间 document 未变（ghost 方案），UI 层通过
//! `build_ghost_delta_positions` 获取被拖动音符的 ghost 位置，
//! 生成 UpdateMany 增量发送替代每帧全量重建。

use super::Editor;
use super::ghost::{has_active_ghost_delta, is_note_ghosted, push_copy_instances};
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

    /// 是否存在待提交的复制（Ctrl+拖动复制 / 待提交副本）
    ///
    /// 副本渲染走预览通道（原件保留在 GPU 段原位），UI 层据此分流。
    pub fn has_pending_copy_drag(&self) -> bool {
        self.pending_copy_drag_state.is_some()
            || matches!(
                self.editor_state.interaction.edit_state,
                EditState::DraggingSelectionCopy { .. }
            )
    }

    /// 构建 ghost 拖动增量的位置数据（仅「选中 ∩ 可见」的索引）
    ///
    /// 统一全量渲染（2026-08-06）：GPU 布局 = 全量轨段，段内位置 = notes 索引，
    /// 因此返回值第一项为 **notes 索引**（而非旧可见列表下标）。
    /// 返回 `(notes 索引, ghost 后位置 (tick, key, length))`（按索引升序），
    /// UI 层据此生成 UpdateMany 增量发送。
    ///
    /// 调用前提：`has_active_ghost_delta` 为 true 且 `visible_indices` 与
    /// 当前视口内索引一致（仅用于过滤被拖音符，无需与 GPU 布局对应）。
    ///
    /// **复制模式例外**：`DraggingSelectionCopy` / `pending_copy_drag_state`
    /// 副本走预览通道（[`Self::build_copy_ghost_positions`]），本方法返回空列表。
    pub fn build_ghost_delta_positions(
        &self,
        visible_indices: &[usize],
    ) -> Vec<(usize, (f32, u16, f32))> {
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;
        let pending_copy = &self.pending_copy_drag_state;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let data = &self.editor_state.data;

        // 批量框选拉伸：不修改 document，根据 origin/last tick 差值实时预览长度
        match edit_state {
            EditState::ResizingSelectionStart {
                origin_tick,
                last_tick,
            } => {
                return self.build_resize_start_ghost_positions(
                    visible_indices,
                    *origin_tick,
                    *last_tick,
                );
            }
            EditState::ResizingSelectionEnd {
                origin_tick,
                last_tick,
            } => {
                return self.build_resize_end_ghost_positions(
                    visible_indices,
                    *origin_tick,
                    *last_tick,
                );
            }
            _ => {}
        }

        let (drag_dt, drag_dk) = current_drag_delta(edit_state);

        // 复制模式：副本实例走预览通道，本方法禁用增量
        let any_copy =
            pending_copy.is_some() || matches!(edit_state, EditState::DraggingSelectionCopy { .. });
        if any_copy {
            return Vec::new();
        }

        let mut out = Vec::new();
        for &note_idx in visible_indices {
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
            out.push((note_idx, (tick, key, length)));
        }
        out
    }

    /// 批量框选左边缘拉伸 ghost 位置
    fn build_resize_start_ghost_positions(
        &self,
        visible_indices: &[usize],
        origin_tick: f32,
        last_tick: f32,
    ) -> Vec<(usize, (f32, u16, f32))> {
        let delta_tick = last_tick - origin_tick;
        if delta_tick == 0.0 {
            return Vec::new();
        }
        let snap_precision = self.editor_state.view.snap_precision;
        let mut out = Vec::new();
        for &i in visible_indices {
            if !self.is_note_selected(i) {
                continue;
            }
            let Some(note) = self.editor_state.data.get_note_view(i) else {
                continue;
            };
            let new_length = note.length - delta_tick;
            if new_length >= snap_precision {
                let new_tick = (note.tick + delta_tick).max(0.0);
                out.push((i, (new_tick, note.key, new_length)));
            }
        }
        out
    }

    /// 批量框选右边缘拉伸 ghost 位置
    fn build_resize_end_ghost_positions(
        &self,
        visible_indices: &[usize],
        origin_tick: f32,
        last_tick: f32,
    ) -> Vec<(usize, (f32, u16, f32))> {
        let delta_tick = last_tick - origin_tick;
        if delta_tick == 0.0 {
            return Vec::new();
        }
        let snap_precision = self.editor_state.view.snap_precision;
        let mut out = Vec::new();
        for &i in visible_indices {
            if !self.is_note_selected(i) {
                continue;
            }
            let Some(note) = self.editor_state.data.get_note_view(i) else {
                continue;
            };
            let new_length = note.length + delta_tick;
            if new_length >= snap_precision {
                out.push((i, (note.tick, note.key, new_length)));
            }
        }
        out
    }

    /// 构建复制模式副本的可见位置（统一全量渲染：副本 → 预览通道）
    ///
    /// 复制模式下原件保留在 GPU 段原位，副本（原位置 + 偏移位置，连续复制
    /// 时旧副本 + 新副本并存）叠加渲染。返回副本位置列表 `(tick, key, length)`
    /// （仅视口内），UI 层构建 NoteInstance 发送 `PreviewInstances`。
    pub fn build_copy_ghost_positions(&self, visible_indices: &[usize]) -> Vec<(f32, u16, f32)> {
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending_copy = &self.pending_copy_drag_state;
        let pending_drag = &self.pending_drag_state;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let data = &self.editor_state.data;

        let mut out = Vec::new();
        for &note_idx in visible_indices {
            let Some((tick, key, length)) = data
                .get_note_view(note_idx)
                .map(|n| (n.tick, n.key, n.length))
            else {
                continue;
            };
            push_copy_instances(
                &mut out,
                tick,
                key,
                length,
                max_key,
                note_idx,
                pending_copy,
                pending_drag,
                edit_state,
                |_, _| true,
            );
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
