//! 鼠标释放事件处理 — 完成编辑操作
//!
//! 包含：释放事件的匹配分发、绘制完成的收尾工作

use crate::drag::compute_state_changes::{
    apply_resize_end_to_selected, apply_resize_start_to_selected,
};
use crate::{EditState, Editor};
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

        // 画刷笔触结束：收尾并清状态（绕过通用 Drawing 收尾）
        if self.brush_last_cell.is_some() {
            self.finish_brush_stroke();
            return;
        }

        match edit_state {
            EditState::Selecting { .. } => {
                if self.editor_state.tool == Tool::Eraser
                    || self.editor_state.tool == Tool::DrawEraser
                {
                    // 框选过程中 update_selection 已维护好 selected_notes，
                    // 直接复用，避免重复线性扫描。
                    self.delete_selected_notes();
                } else {
                    tracing::debug!(
                        "框选结束，选中 {} 个音符",
                        self.editor_state.interaction.selected_notes.len()
                    );
                    // 广播本地选择变更（供协作对端高亮 + first-writer-wins 冲突判定）
                    self.emit_local_selection_changed(true);
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
                // 复制模式（ghost 方案，**松手即提交**）：副本立即写入内存层，
                // 不再延迟到点击空白处（BUG 修复：复制后滚动/切换视图副本
                // "消失"、内存无数据——用户看到副本放置成功，必须内存同步
                // 真实化，否则副本只存在于 UI 层 pending，未提交即丢失）。
                //
                // 连续复制（复制下一份）：副本已真实化并**只选中副本**
                // （commit_pending_copy 按参数重选），第二次 Ctrl+拖动副本框时
                // `DragState.selected = 副本`，新副本从副本位置继续偏移——
                // 语义与旧「pending 累积」一致且更直观（每次复制独立可撤销）。
                if drag_state.is_delta_zero() {
                    tracing::debug!("Editor: 复制拖动 delta 为零，取消复制");
                } else {
                    self.pending_copy_drag_state = Some(drag_state);
                    self.commit_pending_copy();
                }
                // 不调用 mark_notes_changed（commit_pending_copy 内部已标记）
            }
            EditState::ResizingSelectionStart {
                origin_tick,
                last_tick,
            } => {
                self.selected_bounds.set(None);
                let delta_tick = last_tick - origin_tick;
                if delta_tick != 0.0 {
                    let selected = self.get_selected_indices();
                    if let Some(track) =
                        self.editor_state.data.document.as_mut().and_then(|doc| {
                            doc.track_notes_mut(self.editor_state.data.current_track)
                        })
                    {
                        apply_resize_start_to_selected(
                            delta_tick,
                            self.editor_state.view.snap_precision,
                            &selected,
                            track,
                        );
                    }
                    self.editor_state.data.record_update_ranges(&selected);
                }
                self.mark_notes_changed();
            }
            EditState::ResizingSelectionEnd {
                origin_tick,
                last_tick,
            } => {
                self.selected_bounds.set(None);
                let delta_tick = last_tick - origin_tick;
                if delta_tick != 0.0 {
                    let selected = self.get_selected_indices();
                    if let Some(track) =
                        self.editor_state.data.document.as_mut().and_then(|doc| {
                            doc.track_notes_mut(self.editor_state.data.current_track)
                        })
                    {
                        apply_resize_end_to_selected(
                            delta_tick,
                            self.editor_state.view.snap_precision,
                            &selected,
                            track,
                        );
                    }
                    self.editor_state.data.record_update_ranges(&selected);
                }
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
