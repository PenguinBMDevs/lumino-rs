//! 鼠标释放事件处理 — 完成编辑操作
//!
//! 包含：释放事件的匹配分发、绘制完成的收尾工作

use crate::{EditState, Editor};
use lumino_editor_state::DragState;
use lumino_editor_state::LineToolInteraction;
use lumino_message::Tool;

impl Editor {
    /// 处理鼠标释放事件
    pub(crate) fn handle_released(&mut self) {
        crate::puffin_profiler::released_handle();
        let edit_state = std::mem::take(&mut self.editor_state.interaction.edit_state);

        // 图片转 MIDI 放置模式：框选完成/移动拉伸结束优先处理
        if self.editor_state.image_to_midi.is_active() {
            self.handle_i2m_released(edit_state);
            return;
        }

        // 曲线工具直线模式：结束锚点/连线拖动
        if self.editor_state.tool == Tool::Curve
            && self.editor_state.line_tool.interaction != LineToolInteraction::None
        {
            self.handle_line_tool_released();
            return;
        }

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
                // 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick/length f32）
                if let Some(note) = self.editor_state.data.get_note_view(note_index)
                    && (note.tick != original_tick || note.length != original_length)
                {
                    self.mark_notes_changed();
                }
            }
            EditState::ResizingEnd {
                note_index,
                original_length,
            } => {
                if let Some(note) = self.editor_state.data.get_note_view(note_index)
                    && note.length != original_length
                {
                    self.mark_notes_changed();
                }
            }
            EditState::DraggingSelection { drag_state } => {
                crate::puffin_profiler::released_dragging_selection();
                // 松手：停止批量拖动预览序列（剩余未弹出的试听音符作废）
                self.editor_state.interaction.clear_preview_sequence();
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
                // 不调用 mark_notes_changed（document 未变）
            }
            EditState::DraggingSelectionCopy { drag_state } => {
                crate::puffin_profiler::released_dragging_selection();
                // 松手：停止批量拖动预览序列（复制拖动同样有发声反馈）
                self.editor_state.interaction.clear_preview_sequence();
                // 复制模式（ghost 方案，延迟提交）：松手不写入 document，
                // 保存到 pending_copy_drag_state——副本持续显示在 UI 层。
                // 用户点击空白处退出框选时才真正写入内存层
                // （flush_pending_drag → commit_pending_copy）。
                //
                // **连续复制（复制下一份，BUG 修复）**：已有 pending_copy 时
                // （第二次及以后 Ctrl+拖动副本框），旧副本**提交入内存**（真实化，
                // 选中 = 原件 ∪ 旧副本），新副本以「相对原件的累积 delta」成为新
                // pending——旧副本保持原位，新副本从旧副本位置继续偏移，两份副本
                // 并存（不再被吞并）。渲染验证见 copy_deltas_for_index。
                if drag_state.is_delta_zero() {
                    tracing::debug!("Editor: 复制拖动 delta 为零，取消复制");
                } else if let Some(pending) = self.pending_copy_drag_state.take() {
                    // 提交前记录原件参数：batch_insert 会位移全局索引，
                    // 提交后须按参数全等重选原件（新索引），下次复制的
                    // pending.selected 才能指向正确的原件。
                    let originals: Vec<crate::Note> = pending
                        .selected_indices_fast()
                        .into_iter()
                        .filter_map(|i| self.editor_state.data.get_note_view(i))
                        .map(|n| {
                            crate::Note::from_raw(n.tick, n.key, n.length, n.velocity, n.channel)
                        })
                        .collect();
                    // 新 pending delta = 旧副本 delta + 本次拖动 delta（相对原件累积）
                    let accum_delta_tick = pending.delta_tick.saturating_add(drag_state.delta_tick);
                    let accum_delta_key = pending.delta_key.saturating_add(drag_state.delta_key);
                    // 旧副本提交入内存（pending 放回供 commit 读取；commit 内部清空）
                    self.pending_copy_drag_state = Some(pending);
                    self.commit_pending_copy();
                    // 提交后按参数重选原件（新索引）：作为**复制锚点**
                    // （pending.selected 决定下次副本渲染基于哪些音符），
                    // 视觉上原件不再框选（复制模式只显示副本框——最新件框选）
                    self.selection_clear();
                    self.select_notes_by_params(&originals);
                    // 新 pending：selected = 原件（新索引），delta = 累积偏移
                    let note_count = self.editor_state.data.current_track_note_count();
                    let mut new_pending =
                        DragState::from_indices(self.get_selected_indices(), note_count, 0, 0);
                    new_pending.set_delta(accum_delta_tick, accum_delta_key);
                    self.pending_copy_drag_state = Some(new_pending);
                    tracing::debug!(
                        "Editor: 连续复制 - 旧副本已提交，新副本累积 delta=({}, {})",
                        accum_delta_tick,
                        accum_delta_key
                    );
                } else {
                    tracing::debug!(
                        "Editor: 保存 pending 复制 - delta=({}, {})",
                        drag_state.delta_tick,
                        drag_state.delta_key
                    );
                    self.pending_copy_drag_state = Some(drag_state);
                }
                // 保留 selected_notes 不清空（pending 状态下仍显示框选）
                // edit_state 切换到 Idle（std::mem::take 已处理）
                // 不调用 mark_notes_changed（document 未变）
            }
            EditState::ResizingSelectionStart { .. } | EditState::ResizingSelectionEnd { .. } => {
                // ghost 方案：期间用 mark_ghost_dirty 不重建索引，松手时一次性重建。
                // 2026-08 单一权威源：resize 期间已直接修改 document（track_notes_mut），
                // 松手时只需标记变化，无需任何缓存同步。
                //
                // 清除 selected_bounds 缓存：拉伸期间虽增量更新，但有个别音符可能因
                // new_length < snap_precision 被跳过，导致缓存与实际不完全一致。
                // 松手后强制 O(N) 回退路径重新计算，确保正确性。
                self.selected_bounds.set(None);
                // NoteStore 已删除（降级 no-op，保留调用兼容）
                if self.editor_state.data.is_note_store_enabled() {
                    self.editor_state.data.sync_notes_from_store();
                }
                tracing::debug!("Editor: 选择框批量编辑完成，重建空间索引");
                // 标记当前轨变化（document 已被直接修改）
                self.editor_state.data.mark_current_track_changed();
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
