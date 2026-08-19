//! 历史操作应用（MoveOp / CreateOp）与 DragState 构造
//!
//! 这些逻辑从 `history.rs` 拆分出来，使主文件保持在 400 行以内。

use lumino_note_core::history::{CreateOp, MoveOp};

use super::EditorData;
use crate::DragState;

impl EditorData {
    /// 应用音符创建日志到 document（增量恢复，单一权威源）
    ///
    /// - `inverse=true`（undo）：按值精确定位（`position_of`）后删除音符，
    ///   顺序无关——不受同轨后续操作导致的索引漂移影响。
    /// - `inverse=false`（redo）：按 tick 有序重新插入，恢复到创建时的位置。
    ///
    /// 返回实际处理的音符数。
    pub fn apply_create_ops(&mut self, ops: &[CreateOp], inverse: bool) -> usize {
        if ops.is_empty() {
            return 0;
        }
        let Some(doc) = self.document.as_mut() else {
            return 0;
        };

        let mut count = 0usize;
        for op in ops {
            let track_id = op.track_id as usize;
            if inverse {
                // undo：按值匹配删除（精确，与合并窗口内插入顺序无关）
                let Some(idx) = doc.track_notes(track_id).position_of(&op.note) else {
                    continue;
                };
                if doc.remove_note(track_id, idx).is_some() {
                    count += 1;
                }
            } else {
                // redo：按 tick 有序插入，恢复创建时的位置
                if doc.insert_note(track_id, op.note) {
                    count += 1;
                }
            }
        }
        count
    }

    /// 应用 MoveOp 列表到 document 对应音轨（单一权威源）。
    ///
    /// `inverse=true` 时按记录的原始位置恢复（用于 undo）。
    /// `max_key` 用于 clamp key 范围（通常传 `visible_key_count - 1`）。
    /// 返回实际修改的音符数。
    pub fn apply_move_ops(&mut self, ops: &[MoveOp], inverse: bool, max_key: u16) -> usize {
        if ops.is_empty() {
            return 0;
        }

        let mut modified = 0usize;
        for op in ops {
            let track_id = op.track_id as usize;
            let start = op.range_start as usize;
            let end = op.range_end as usize;

            let Some(track) = self
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(track_id))
            else {
                continue;
            };

            if inverse {
                // 按原始位置恢复，确保 clamp 后的音符也能精确还原
                for (idx, i) in (start..end).enumerate() {
                    if idx >= op.original_ticks.len() || idx >= op.original_keys.len() {
                        break;
                    }
                    let orig_tick = op.original_ticks[idx];
                    let orig_key = op.original_keys[idx];
                    if let Some(note) = track.get_mut(i)
                        && (note.start_tick as f32 != orig_tick || note.key != orig_key as u8)
                    {
                        // 移动不改变长度：恢复 start 时按当前 length 平移 end
                        // （forward 保证 end 跟随 start 平移，length 不变式成立）
                        let length = note.end_tick.saturating_sub(note.start_tick).max(1);
                        note.start_tick = super::super::accessors::f32_to_tick(orig_tick);
                        note.end_tick = note.start_tick.saturating_add(length);
                        note.key = orig_key as u8;
                        modified += 1;
                    }
                }
            } else {
                let dt = op.delta_tick;
                let dk = op.delta_key as i32;
                for i in start..end {
                    if let Some(note) = track.get_mut(i) {
                        let new_tick = (note.start_tick as i64 + dt as i64).max(0) as u32;
                        let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u8;
                        if note.start_tick != new_tick || note.key != new_key {
                            note.start_tick = new_tick;
                            // 移动不改变长度：end_tick 跟随 start_tick 平移
                            let new_end =
                                (note.end_tick as i64 + dt as i64).max(new_tick as i64 + 1) as u32;
                            note.end_tick = new_end;
                            note.key = new_key;
                            modified += 1;
                        }
                    }
                }
            }
        }

        if modified > 0 {
            // 记录所有受影响音轨：若全部是当前音轨（洋葱皮不显示），
            // stream_onion_skin_instances 可豁免全量重建上传。
            let dirty_tracks: std::collections::HashSet<usize> =
                ops.iter().map(|op| op.track_id as usize).collect();
            self.mark_track_notes_changed_for(Some(dirty_tracks));
        }
        modified
    }

    /// 当前 view 下可用于 clamp key 的最大 key 索引
    ///
    /// EditorData 本身不持有 view，默认用 255（MIDI 最大 key）。
    /// UI 层调用 `apply_move_ops` 时应传入实际 `visible_key_count - 1`。
    pub(crate) fn max_key_for_move_op(&self) -> u16 {
        255
    }

    /// 从 DragState 构造 MoveOp 列表（按连续区间拆分）。
    ///
    /// **优化**：`selected_indices()` 已按索引升序返回，无需 sort。
    /// NoteStore 启用时用 `get_ref`（Copy 语义）替代 `notes.get`（clone Note）。
    pub fn move_ops_from_drag_state(&self, drag_state: &DragState) -> Vec<MoveOp> {
        let track_id = self.current_track as u32;
        let indices: Vec<usize> = drag_state.selected_indices();
        if indices.is_empty() {
            return Vec::new();
        }
        // selected_indices() 已升序，无需 sort

        let delta_tick = SaturatingInto::<i32>::saturating_into(drag_state.delta_tick);
        let delta_key = drag_state.delta_key;

        let mut ops = Vec::new();
        let mut seq = 0u16;
        let mut range_start = indices[0];
        let mut prev = indices[0];

        // 直接遍历 document 当前轨提取原始 tick/key（单一权威源）
        let track_notes = self.current_track_notes();
        let make_op = |start: usize, end: usize, seq: u16| {
            let (ticks, keys): (Vec<f32>, Vec<u16>) = track_notes
                .get_range(start..=end)
                .map(|slice| {
                    slice
                        .iter()
                        .map(|note| (note.start_tick as f32, note.key as u16))
                        .unzip()
                })
                .unwrap_or_default();
            MoveOp {
                track_id,
                range_start: start as u32,
                range_end: (end + 1) as u32,
                delta_tick,
                delta_key,
                seq,
                original_ticks: ticks,
                original_keys: keys,
            }
        };

        for &note_idx in &indices[1..] {
            if note_idx == prev + 1 {
                prev = note_idx;
            } else {
                ops.push(make_op(range_start, prev, seq));
                seq = seq.wrapping_add(1);
                range_start = note_idx;
                prev = note_idx;
            }
        }
        // 最后一段
        ops.push(make_op(range_start, prev, seq));

        ops
    }
}

/// i64 饱和转换到 i32 的辅助 trait
pub(crate) trait SaturatingInto<T> {
    /// 饱和转换
    fn saturating_into(self) -> T;
}

impl SaturatingInto<i32> for i64 {
    fn saturating_into(self) -> i32 {
        self.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }
}
