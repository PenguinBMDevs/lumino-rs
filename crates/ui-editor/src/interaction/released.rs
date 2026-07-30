//! 鼠标释放事件处理 — 完成编辑操作
//!
//! 包含：释放事件的匹配分发、绘制完成的收尾工作

use crate::{EditState, Editor};
use lumino_message::Tool;

impl Editor {
    /// 处理鼠标释放事件
    pub(crate) fn handle_released(&mut self) {
        crate::puffin_profiler::released_handle();
        let edit_state = std::mem::take(&mut self.editor_state.interaction.edit_state);
        match edit_state {
            EditState::Selecting { .. } => {
                if self.editor_state.tool == Tool::Eraser {
                    // 框选过程中 update_selection 已维护好 selected_notes，
                    // 直接复用，避免重复线性扫描。
                    self.delete_selected_notes();
                } else {
                    tracing::debug!(
                        "框选结束，选中 {} 个音符",
                        self.editor_state.interaction.selected_notes.len()
                    );
                }
            }
            EditState::Drawing {
                start_tick,
                key,
                current_tick,
            } => {
                self.finish_drawing(start_tick, key, current_tick);
            }
            EditState::PendingDrag { .. } => {}
            EditState::Dragging {
                note_index,
                drag_state,
                ..
            } => {
                // ghost 方案：松手时一次性应用 delta 到 notes
                if self.finalize_dragging(note_index, drag_state) {
                    self.mark_notes_changed();
                }
            }
            EditState::ResizingStart {
                note_index,
                original_tick,
                original_length,
            } => {
                if let Some(note) = self.editor_state.data.notes.get(note_index)
                    && (note.tick != original_tick || note.length != original_length)
                {
                    self.mark_notes_changed();
                }
            }
            EditState::ResizingEnd {
                note_index,
                original_length,
            } => {
                if let Some(note) = self.editor_state.data.notes.get(note_index)
                    && note.length != original_length
                {
                    self.mark_notes_changed();
                }
            }
            EditState::DraggingSelection { drag_state } => {
                crate::puffin_profiler::released_dragging_selection();
                // ghost 方案（延迟提交）：松手不 apply，保存到 pending_drag_state。
                // 用户点击空白处取消框选时才 apply（commit_pending_drag）。
                //
                // 累积模式：如果已有 pending_drag_state，新 delta 叠加到 pending.delta。
                // 渲染时 ghost 位置 = note + pending.delta + drag_state.delta。
                if drag_state.is_delta_zero() {
                    tracing::debug!("Editor: 批量拖动 delta 为零，不保存 pending");
                } else if let Some(mut pending) = self.pending_drag_state.take() {
                    pending.delta_tick = pending.delta_tick.saturating_add(drag_state.delta_tick);
                    pending.delta_key = pending.delta_key.saturating_add(drag_state.delta_key);
                    tracing::debug!(
                        "Editor: 累积 pending 拖动 - 累积 delta=({}, {})",
                        pending.delta_tick,
                        pending.delta_key
                    );
                    self.pending_drag_state = Some(pending);
                } else {
                    tracing::debug!(
                        "Editor: 保存 pending 拖动 - delta=({}, {})",
                        drag_state.delta_tick,
                        drag_state.delta_key
                    );
                    self.pending_drag_state = Some(drag_state);
                }
                // 保留 selected_notes 不清空（pending 状态下仍显示框选）
                // edit_state 切换到 Idle（std::mem::take 已处理）
                // 不调用 mark_notes_changed（data.notes 未变）
            }
            EditState::ResizingSelectionStart { .. } | EditState::ResizingSelectionEnd { .. } => {
                // ghost 方案：期间用 mark_ghost_dirty 不重建索引，松手时一次性重建。
                // notes 已在 drag.rs 每帧被改，此处只把发生变更的选中音符流式同步到
                // track_notes，避免整轨克隆。
                //
                // 清除 selected_bounds 缓存：拉伸期间虽增量更新，但有个别音符可能因
                // new_length < snap_precision 被跳过，导致缓存与实际不完全一致。
                // 松手后强制 O(N) 回退路径重新计算，确保正确性。
                self.selected_bounds.set(None);
                // NoteStore 启用时，拉伸期间修改的是 note_store 而非 notes，
                // 需要同步回 notes 确保一致性（sync_track_notes_at_indices 读取 notes）。
                if self.editor_state.data.is_note_store_enabled() {
                    self.editor_state.data.sync_notes_from_store();
                }
                tracing::debug!("Editor: 选择框批量编辑完成，重建空间索引");
                let selected = self.get_selected_indices();
                self.editor_state
                    .data
                    .sync_track_notes_at_indices(&selected);
                self.mark_notes_changed();
            }
            _ => {}
        }
    }

    /// 完成绘制新音符
    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let v = &self.editor_state.view;
        // 默认音符长度优先使用上次放置的长度，其次使用精度设置的默认长度
        let effective_default_length = v.last_note_length.unwrap_or(v.default_note_length);
        if let Some(note) = self.editor_state.data.finish_drawing(
            start_tick,
            key,
            current_tick,
            v.snap_precision,
            effective_default_length,
        ) {
            // 保存本次放置的音符长度，作为下次预览和放置的默认长度
            self.editor_state.view.set_last_note_length(note.length);
            self.emit_note_added_event(&note);
            self.mark_notes_changed();
        }
    }
}
