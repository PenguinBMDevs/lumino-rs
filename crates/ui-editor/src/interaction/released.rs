//! 鼠标释放事件处理 — 完成编辑操作
//!
//! 包含：释放事件的匹配分发、绘制完成的收尾工作

use crate::{EditState, Editor};
use lumino_message::Tool;

impl Editor {
    /// 处理鼠标释放事件
    pub(crate) fn handle_released(&mut self) {
        let edit_state = std::mem::take(&mut self.editor_state.interaction.edit_state);
        match edit_state {
            EditState::Selecting {
                start_tick,
                start_key,
                current_tick,
                current_key,
            } => {
                if self.editor_state.tool == Tool::Eraser {
                    self.delete_notes_in_selection_box(
                        start_tick,
                        start_key,
                        current_tick,
                        current_key,
                    );
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
                original_tick,
                original_key,
                ..
            } => {
                if self.finalize_dragging(note_index, original_tick, original_key) {
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
            EditState::DraggingSelection { .. }
            | EditState::ResizingSelectionStart { .. }
            | EditState::ResizingSelectionEnd { .. } => {
                tracing::debug!("Editor: 选择框批量编辑完成");
            }
            _ => {}
        }
    }

    /// 完成绘制新音符
    pub(crate) fn finish_drawing(&mut self, start_tick: f32, key: u16, current_tick: f32) {
        let v = &self.editor_state.view;
        if let Some(note) = self.editor_state.data.finish_drawing(
            start_tick,
            key,
            current_tick,
            v.snap_precision,
            v.default_note_length,
        ) {
            self.emit_note_added_event(&note);
            self.mark_notes_changed();
        }
    }
}
