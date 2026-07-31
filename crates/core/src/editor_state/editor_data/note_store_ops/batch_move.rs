//! 批量移动音符操作（NoteStore 并行热路径 + 冷路径回退）

use super::super::EditorData;
use super::sync::bitvec_to_bitset;
use crate::note_store::BitSet;
use crate::DragState;

impl EditorData {
    /// 批量移动选中音符（NoteStore 并行热路径）
    ///
    /// 当 NoteStore 启用时走 `batch_move_parallel`（8 线程并行，16M 50% 18ms），
    /// 否则回退到直接遍历 notes。
    ///
    /// 返回实际修改的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_move_notes(
        &mut self,
        selected: &BitSet,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        if self.note_store_enabled {
            let modified = self
                .note_store
                .batch_move_parallel(selected, delta_tick, delta_key, max_key);
            self.sync_notes_from_store();
            self.sync_track_notes();
            tracing::debug!(
                "NoteStore 批量移动: 修改 {} 音符, 选中 {}",
                modified,
                selected.count_ones()
            );
            modified
        } else {
            let mut modified = 0usize;
            for note_idx in 0..self.notes.len() {
                if selected.get(note_idx)
                    && let Some(note) = self.notes.get_mut(note_idx)
                {
                    let new_tick = (note.tick + delta_tick).max(0.0);
                    let new_key =
                        (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            if modified > 0 {
                self.sync_track_notes();
            }
            modified
        }
    }

    /// 批量移动选中音符——**不同步 im::Vector**（NoteStore 热路径专用）
    ///
    /// 调用方必须在**渲染前**手动调用 `sync_notes_from_store()` 确保一致性。
    pub fn batch_move_notes_no_sync(
        &mut self,
        selected: &BitSet,
        delta_tick: f32,
        delta_key: i16,
        max_key: u16,
    ) -> usize {
        if selected.count_ones() == 0 {
            return 0;
        }

        if self.note_store_enabled {
            self.note_store
                .batch_move_parallel(selected, delta_tick, delta_key, max_key)
        } else {
            let mut modified = 0usize;
            for note_idx in 0..self.notes.len() {
                if selected.get(note_idx)
                    && let Some(note) = self.notes.get_mut(note_idx)
                {
                    let new_tick = (note.tick + delta_tick).max(0.0);
                    let new_key =
                        (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            modified
        }
    }

    /// 从 DragState 批量移动选中音符（集成层适配）
    ///
    /// 返回修改的音符数。调用方需在调用前 `push_history()`。
    pub fn batch_move_notes_from_drag_state(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        let bitset = bitvec_to_bitset(&drag_state.selected);
        self.batch_move_notes(
            &bitset,
            drag_state.delta_tick as f32,
            drag_state.delta_key,
            max_key,
        )
    }

    /// 从 DragState 批量移动选中音符——**不同步 im::Vector**
    ///
    /// 适用场景：`commit_pending_drag` 等高频热路径。
    pub fn batch_move_notes_from_drag_state_no_sync(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        let bitset = bitvec_to_bitset(&drag_state.selected);
        self.batch_move_notes_no_sync(
            &bitset,
            drag_state.delta_tick as f32,
            drag_state.delta_key,
            max_key,
        )
    }

    /// 从 DragState 批量移动选中音符——**直接接受 &BitVec，消除 BitVec→BitSet 转换**
    ///
    /// 适用场景：`commit_pending_drag` 等高频热路径。
    pub fn batch_move_notes_from_bitvec_no_sync(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        if self.note_store_enabled {
            self.note_store.batch_move_parallel_from_bitvec(
                &drag_state.selected,
                drag_state.delta_tick as f32,
                drag_state.delta_key,
                max_key,
            )
        } else {
            let mut modified = 0usize;
            for (note_idx, selected) in drag_state.selected.iter().enumerate() {
                if !selected || note_idx >= self.notes.len() {
                    continue;
                }
                if let Some(note) = self.notes.get_mut(note_idx) {
                    let new_tick = (note.tick + drag_state.delta_tick as f32).max(0.0);
                    let new_key = (note.key as i32 + drag_state.delta_key as i32)
                        .clamp(0, max_key as i32) as u16;
                    if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                        note.tick = new_tick;
                        note.key = new_key;
                        modified += 1;
                    }
                }
            }
            modified
        }
    }
}
