//! 批量编辑音符属性（力度、长度、key、tick）（降级兼容层）
//!
//! NoteStore SoA 热路径已删除，统一走 document 当前轨操作。
//! 保留签名兼容下游调用。

use std::collections::HashSet;

use super::super::EditorData;
use lumino_note_core::batch_edit::parse_batch_edit_input;

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

        let mut modified = 0usize;
        let mut modified_indices: Vec<usize> = Vec::new();
        let mut transitions: Vec<(lumino_midi_model::NoteEvent, lumino_midi_model::NoteEvent)> =
            Vec::new();
        if let Some(track) = self
            .document
            .as_mut()
            .and_then(|doc| doc.track_notes_mut(self.current_track))
        {
            for &note_idx in selected {
                if let Some(note) = track.get_mut(note_idx) {
                    let old = *note;
                    let mut changed = false;
                    if let Some(op) = velocity_op {
                        let new_v = op.apply(note.velocity as f32).clamp(0.0, 127.0) as u8;
                        if note.velocity != new_v {
                            note.velocity = new_v;
                            changed = true;
                        }
                    }
                    if let Some(op) = gate_op {
                        let current_length = (note.end_tick - note.start_tick) as f32;
                        let new_l = op.apply(current_length).max(1.0);
                        if (current_length - new_l).abs() > f32::EPSILON {
                            note.end_tick = note.start_tick + new_l as u32;
                            changed = true;
                        }
                    }
                    if let Some(op) = key_op {
                        let new_k = op.apply(note.key as f32).clamp(0.0, max_key as f32) as u8;
                        if note.key != new_k {
                            note.key = new_k;
                            changed = true;
                        }
                    }
                    if let Some(op) = tick_op {
                        let new_t = op.apply(note.start_tick as f32).max(0.0);
                        let new_tick = super::super::accessors::f32_to_tick(new_t);
                        if note.start_tick != new_tick {
                            note.end_tick = note.end_tick.max(new_tick.saturating_add(1));
                            note.start_tick = new_tick;
                            changed = true;
                        }
                    }
                    if changed {
                        modified += 1;
                        modified_indices.push(note_idx);
                        transitions.push((old, *note));
                    }
                }
            }
        }

        self.push_collab_transform_transitions(transitions);

        if modified > 0 {
            self.record_update_ranges(&modified_indices);
        } else {
            self.history.discard_last();
        }
        modified
    }
}
