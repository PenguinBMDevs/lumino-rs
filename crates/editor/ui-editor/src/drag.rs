//! 拖动和调整大小操作

mod compute_state_changes;
mod dragging;
mod selection;

use super::{EditState, Editor};
use compute_state_changes::*;
use lumino_editor_state::PreviewSequenceNote;

impl Editor {
    /// 根据编辑状态计算音符变化量 → (new_tick, new_key, new_length, note_to_play)
    pub(crate) fn compute_state_changes(
        &mut self,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>, Option<u16>) {
        crate::puffin_profiler::compute_state_changes();
        let v = &self.editor_state.view;
        let snap_precision = v.snap_precision;
        let visible_key_count = v.visible_key_count;
        if matches!(
            self.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
        ) {
            self.update_selection();
            return (None, None, None, None);
        }
        let sel: Vec<usize> = {
            let i = &self.editor_state.interaction;
            if let Some(ref bs) = i.selection_bitset {
                let mut v = Vec::with_capacity(bs.count_ones());
                bs.for_each_set(|i| v.push(i));
                v
            } else {
                i.selected_notes.iter().copied().collect()
            }
        };
        let (mut new_tick, mut new_length, mut note_to_play) = (None, None, None);
        // 批量拖动预览序列信号：None=key 偏移无变化；Some(空)=回到原位需清空；
        // Some(非空)=按 tick 顺序 + ghost key 构建的新序列。
        let mut preview_signal: Option<Vec<PreviewSequenceNote>> = None;
        match &mut self.editor_state.interaction.edit_state {
            EditState::Drawing { current_tick, .. } => handle_drawing(current_tick, snapped_tick),
            EditState::Dragging {
                note_index,
                drag_state,
                last_played_key,
            } => {
                // 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick f32/key u16）
                let orig = self
                    .editor_state
                    .data
                    .get_note_view(*note_index)
                    .map(|n| (n.tick, n.key));
                note_to_play = handle_dragging(
                    drag_state,
                    last_played_key,
                    tick,
                    key,
                    snap_precision,
                    visible_key_count,
                    &orig,
                );
            }
            EditState::ResizingStart {
                original_tick,
                original_length,
                ..
            } => {
                (new_tick, new_length) = handle_resizing_start(
                    *original_tick,
                    *original_length,
                    snapped_tick,
                    snap_precision,
                );
            }
            EditState::ResizingEnd { note_index, .. } => {
                new_length = handle_resizing_end(
                    self.editor_state.data.current_track_notes(),
                    *note_index,
                    snapped_tick,
                    snap_precision,
                );
            }
            EditState::DraggingSelection { drag_state }
            | EditState::DraggingSelectionCopy { drag_state } => {
                // 复制拖动：偏移计算与移动拖动一致（原始音符不动，副本按 delta 渲染）。
                // key 偏移变化 → 触发/停止批量拖动预览序列（发声反馈）：
                // 按选中音符的 tick 顺序 + 当前 ghost key 位置 + BPM 时序构建。
                if let Some(new_delta_key) =
                    handle_dragging_selection(drag_state, key, snapped_tick, snap_precision)
                {
                    preview_signal = Some(if new_delta_key == 0 {
                        Vec::new()
                    } else {
                        build_preview_sequence(
                            &self.editor_state.data,
                            drag_state,
                            new_delta_key,
                            visible_key_count.saturating_sub(1),
                            std::time::Instant::now(),
                            DEFAULT_NOTE_VELOCITY,
                        )
                    });
                }
            }
            EditState::ResizingSelectionStart { last_tick } => {
                // 2026-08 单一权威源：直接修改 document 当前轨（track_notes_mut）
                if let Some(track) = self
                    .editor_state
                    .data
                    .document
                    .as_mut()
                    .and_then(|doc| doc.track_notes_mut(self.editor_state.data.current_track))
                    && handle_resizing_selection_start(
                        last_tick,
                        snapped_tick,
                        snap_precision,
                        &sel,
                        track,
                        &self.selected_bounds,
                    )
                {
                    self.mark_ghost_dirty();
                }
            }
            EditState::ResizingSelectionEnd { last_tick } => {
                // 2026-08 单一权威源：直接修改 document 当前轨（track_notes_mut）
                if let Some(track) = self
                    .editor_state
                    .data
                    .document
                    .as_mut()
                    .and_then(|doc| doc.track_notes_mut(self.editor_state.data.current_track))
                    && handle_resizing_selection_end(
                        last_tick,
                        snapped_tick,
                        snap_precision,
                        &sel,
                        track,
                        &self.selected_bounds,
                    )
                {
                    self.mark_ghost_dirty();
                }
            }
            _ => {}
        }
        // match 借用结束后统一处理预览序列（避免与 edit_state 的可变借用冲突）
        if let Some(signal) = preview_signal {
            let interaction = &mut self.editor_state.interaction;
            if signal.is_empty() {
                interaction.clear_preview_sequence();
            } else {
                interaction.set_preview_sequence(signal);
            }
        }
        (new_tick, None, new_length, note_to_play)
    }
}

use lumino_ui_core::constants::editor::DEFAULT_NOTE_VELOCITY;
