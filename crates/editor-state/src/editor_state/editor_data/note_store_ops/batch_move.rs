//! 批量移动音符操作（降级兼容层）
//!
//! NoteStore 并行热路径已删除，统一走 im::Vector 冷路径。
//! 保留全部签名兼容下游调用；第二阶段由 MidiDocument 分块接管批量移动。

use super::super::EditorData;
use crate::DragState;
use lumino_note_core::note_store::BitSet;

impl EditorData {
    /// 批量移动选中音符
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

        let mut modified = 0usize;
        let mut modified_indices: Vec<usize> = Vec::new();
        for note_idx in 0..self.notes.len() {
            if selected.get(note_idx)
                && let Some(note) = self.notes.get_mut(note_idx)
            {
                let new_tick = (note.tick + delta_tick).max(0.0);
                let new_key = (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                    note.tick = new_tick;
                    note.key = new_key;
                    modified += 1;
                    modified_indices.push(note_idx);
                }
            }
        }
        if modified > 0 {
            self.record_update_ranges(&modified_indices);
        }
        modified
    }

    /// 批量移动选中音符——**不同步 track_notes**（热路径专用）
    ///
    /// 调用方必须自行保证后续一致性（如显式 `sync_track_notes`）。
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

        let mut modified = 0usize;
        for note_idx in 0..self.notes.len() {
            if selected.get(note_idx)
                && let Some(note) = self.notes.get_mut(note_idx)
            {
                let new_tick = (note.tick + delta_tick).max(0.0);
                let new_key = (note.key as i32 + delta_key as i32).clamp(0, max_key as i32) as u16;
                if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                    note.tick = new_tick;
                    note.key = new_key;
                    modified += 1;
                }
            }
        }
        modified
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
        let bitset = sync_bitvec_to_bitset(&drag_state.selected);
        self.batch_move_notes(
            &bitset,
            drag_state.delta_tick as f32,
            drag_state.delta_key,
            max_key,
        )
    }

    /// 从 DragState 批量移动选中音符——**不同步 track_notes**
    pub fn batch_move_notes_from_drag_state_no_sync(
        &mut self,
        drag_state: &DragState,
        max_key: u16,
    ) -> usize {
        if drag_state.is_delta_zero() || !drag_state.has_selection() {
            return 0;
        }
        let bitset = sync_bitvec_to_bitset(&drag_state.selected);
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
        let mut modified = 0usize;
        for (note_idx, selected) in drag_state.selected.iter().enumerate() {
            if !selected || note_idx >= self.notes.len() {
                continue;
            }
            if let Some(note) = self.notes.get_mut(note_idx) {
                let new_tick = (note.tick + drag_state.delta_tick as f32).max(0.0);
                let new_key =
                    (note.key as i32 + drag_state.delta_key as i32).clamp(0, max_key as i32) as u16;
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

/// BitVec → BitSet 转换（原 sync.rs，迁入本文件避免模块循环）
pub(super) fn sync_bitvec_to_bitset(bv: &bit_vec::BitVec) -> BitSet {
    let len = bv.len();
    let mut selected_bits = BitSet::new(len);
    for (block_idx, block) in bv.blocks().enumerate() {
        if block == 0 {
            continue;
        }
        let base = block_idx * 64;
        let mut bits = block;
        while bits != 0 {
            let tz = bits.trailing_zeros() as usize;
            let idx = base + tz;
            if idx < len {
                selected_bits.set(idx);
            }
            bits &= bits - 1;
        }
    }
    selected_bits
}
