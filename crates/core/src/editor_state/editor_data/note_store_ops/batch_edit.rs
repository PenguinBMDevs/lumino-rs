//! 批量编辑音符属性（力度、长度、key、tick）

use std::collections::HashSet;

use super::super::EditorData;
use crate::batch_edit::parse_batch_edit_input;
use crate::batch_edit::BatchEditOperation;
use crate::note_store::BitSet;

impl EditorData {
    /// 批量编辑选中音符的力度、长度、key、tick
    ///
    /// 对 `selected` 中的每个索引，依次应用四个字段的批量编辑运算。
    /// 空字符串表示该字段无操作。返回实际被修改的音符数（至少一个字段被修改）。
    ///
    /// - 力度限制在 0-127
    /// - key 限制在 0-max_key（128key 模式为 127，256key 模式为 255）
    /// - 长度最小为 1.0
    /// - tick 最小为 0.0
    pub fn apply_batch_edit(
        &mut self,
        selected: &HashSet<usize>,
        velocity: &str,
        gate: &str,
        key: &str,
        tick: &str,
        max_key: u16,
    ) -> usize {
        if selected.is_empty() {
            return 0;
        }

        let velocity_op = parse_batch_edit_input(velocity);
        let gate_op = parse_batch_edit_input(gate);
        let key_op = parse_batch_edit_input(key);
        let tick_op = parse_batch_edit_input(tick);

        if velocity_op.is_none() && gate_op.is_none() && key_op.is_none() && tick_op.is_none() {
            return 0;
        }

        self.push_history();

        let modified = if self.note_store_enabled {
            self.apply_batch_edit_hot_path(selected, velocity_op, gate_op, key_op, tick_op, max_key)
        } else {
            self.apply_batch_edit_cold_path(selected, velocity_op, gate_op, key_op, tick_op, max_key)
        };

        if modified > 0 {
            self.sync_track_notes();
        } else {
            self.history.discard_last();
        }
        modified
    }

    /// NoteStore 热路径：通过 SoA batch_edit_* 方法批量编辑
    fn apply_batch_edit_hot_path(
        &mut self,
        selected: &HashSet<usize>,
        velocity_op: Option<BatchEditOperation>,
        gate_op: Option<BatchEditOperation>,
        key_op: Option<BatchEditOperation>,
        tick_op: Option<BatchEditOperation>,
        max_key: u16,
    ) -> usize {
        let bitset = BitSet::from_iter(self.note_store.len(), selected.iter().copied());
        let mut modified = 0usize;
        if let Some(op) = velocity_op {
            modified += self.note_store.batch_edit_velocity(&bitset, op);
        }
        if let Some(op) = gate_op {
            modified += self.note_store.batch_edit_gate(&bitset, op);
        }
        if let Some(op) = key_op {
            modified += self.note_store.batch_edit_key(&bitset, op, max_key);
        }
        if let Some(op) = tick_op {
            modified += self.note_store.batch_edit_tick(&bitset, op);
        }
        self.sync_notes_from_store();
        modified
    }

    /// 冷路径：直接遍历 im::Vector 逐个编辑
    fn apply_batch_edit_cold_path(
        &mut self,
        selected: &HashSet<usize>,
        velocity_op: Option<BatchEditOperation>,
        gate_op: Option<BatchEditOperation>,
        key_op: Option<BatchEditOperation>,
        tick_op: Option<BatchEditOperation>,
        max_key: u16,
    ) -> usize {
        let mut modified = 0usize;
        for &note_idx in selected {
            if let Some(note) = self.notes.get_mut(note_idx) {
                let mut changed = false;
                if let Some(op) = velocity_op {
                    let new_v = op.apply(note.velocity as f32).clamp(0.0, 127.0) as u8;
                    if note.velocity != new_v {
                        note.velocity = new_v;
                        changed = true;
                    }
                }
                if let Some(op) = gate_op {
                    let new_l = op.apply(note.length).max(1.0);
                    if (note.length - new_l).abs() > f32::EPSILON {
                        note.length = new_l;
                        changed = true;
                    }
                }
                if let Some(op) = key_op {
                    let new_k = op.apply(note.key as f32).clamp(0.0, max_key as f32) as u16;
                    if note.key != new_k {
                        note.key = new_k;
                        changed = true;
                    }
                }
                if let Some(op) = tick_op {
                    let new_t = op.apply(note.tick).max(0.0);
                    if (note.tick - new_t).abs() > f32::EPSILON {
                        note.tick = new_t;
                        changed = true;
                    }
                }
                if changed {
                    modified += 1;
                }
            }
        }
        modified
    }
}
