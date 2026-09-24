//! 历史操作应用（MoveOp / CreateOp）与 DragState 构造
//!
//! 这些逻辑从 `history.rs` 拆分出来，使主文件保持在 400 行以内。
//!
//! 2026-09 统一身份：历史条目一律以 note id 引用音符（`position_of_id` 定位），
//! 不再依赖易漂移的全局索引区间——id 单调分配、删除不回收，永不失效。

use std::collections::{HashMap, HashSet};

use lumino_midi_model::NoteEvent;
use lumino_note_core::Note;
use lumino_note_core::history::{CreateOp, MoveOp};

use super::EditorData;
use crate::DragState;

impl EditorData {
    /// 应用音符创建日志到 document（增量恢复，单一权威源）
    ///
    /// - `inverse=true`（undo）：按全局唯一 id 精确定位删除；`id == 0`
    ///   （兼容旧构造）回退为「音乐内容」全值匹配。顺序无关——不受同轨
    ///   后续操作导致的索引漂移影响。
    /// - `inverse=false`（redo）：按 tick 有序重新插入，**原样保留 op 记录的 id**
    ///   （身份稳定，不再重新分配）。
    ///
    /// 返回实际处理的音符数。
    pub fn apply_create_ops(&mut self, ops: &[CreateOp], inverse: bool) -> usize {
        if ops.is_empty() {
            return 0;
        }
        let mut count = 0usize;
        for op in ops {
            let track_id = op.track_id as usize;
            if inverse {
                let Some(idx) = self.locate_create_op_note(track_id, op) else {
                    continue;
                };
                if self.remove_note(track_id, idx).is_some() {
                    count += 1;
                }
            } else {
                // redo：按 tick 有序重新插入，复用普通插入增量通道（InsertAt）。
                if self.insert_note(track_id, super::super::accessors::event_to_note(&op.note)) {
                    count += 1;
                }
            }
        }
        count
    }

    /// 定位 CreateOp 对应音符的当前索引。
    ///
    /// 优先按全局唯一 id（tick 提示二分 + 同 tick 窗口 + 全扫兜底，O(log N)）；
    /// `id == 0`（旧构造）回退全值匹配（忽略 id——id 由分配器后加，按全值匹配会落空）。
    fn locate_create_op_note(&self, track_id: usize, op: &CreateOp) -> Option<usize> {
        let track = self.track_notes(track_id);
        if op.note.id != NoteEvent::UNASSIGNED_ID {
            return track.position_of_id(op.note.id, op.note.start_tick);
        }
        track.iter().position(|n| {
            n.start_tick == op.note.start_tick
                && n.end_tick == op.note.end_tick
                && n.key == op.note.key
                && n.velocity == op.note.velocity
                && n.channel == op.note.channel
        })
    }

    /// 应用 MoveOp 列表到 document 对应音轨（单一权威源）。
    ///
    /// 每个被移动音符按**全局唯一 id** 定位（tick 提示二分 + 同 tick 窗口 +
    /// 全扫兜底），不依赖提交时的索引——协作远端插入/删除导致的索引漂移
    /// 不再损坏无关音符。
    ///
    /// `inverse=true` 时按记录的原始位置恢复（用于 undo）。
    /// `max_key` 用于 clamp key 范围（通常传 `visible_key_count - 1`）。
    /// 返回实际修改的音符数。
    pub fn apply_move_ops(&mut self, ops: &[MoveOp], inverse: bool, max_key: u16) -> usize {
        if ops.is_empty() {
            return 0;
        }

        let mut modified = 0usize;
        // (track_id, index)：供 UpdateRange 增量事件按实际修改位置生成
        let mut modified_indices: Vec<(usize, usize)> = Vec::new();
        // 就地改 tick 后发生重排的音轨（区间事件按旧索引失效）
        let mut reordered_tracks: HashSet<usize> = HashSet::new();

        for op in ops {
            let track_id = op.track_id as usize;
            let count = op
                .ids
                .len()
                .min(op.original_ticks.len())
                .min(op.original_keys.len());
            if count == 0 {
                continue;
            }
            let Some(track) = self
                .document
                .as_mut()
                .and_then(|doc| doc.track_notes_mut(track_id))
            else {
                continue;
            };

            // 阶段 1：解析全部目标索引（列表仍有序，命中 tick 提示快路径）。
            // 期望当前位置：undo 时 entry 的 delta 已取反 → 当前 = original - delta；
            // redo 时 entry 为正向 op → 当前 = original。
            let mut resolved: Vec<(usize, usize)> = Vec::with_capacity(count);
            for i in 0..count {
                let hint_tick_f = if inverse {
                    (op.original_ticks[i] - op.delta_tick as f32).max(0.0)
                } else {
                    op.original_ticks[i]
                };
                let hint_tick = super::super::accessors::f32_to_tick(hint_tick_f);
                if let Some(idx) = track.position_of_id(op.ids[i], hint_tick) {
                    resolved.push((i, idx));
                }
            }

            // 阶段 2：原地应用（undo 用 original 精确还原，redo 用 delta 前进）。
            let mut applied: Vec<usize> = Vec::with_capacity(resolved.len());
            for (i, idx) in resolved {
                let Some(note) = track.get_mut(idx) else {
                    continue;
                };
                if inverse {
                    let orig_tick = op.original_ticks[i];
                    let orig_key = op.original_keys[i];
                    if note.start_tick as f32 != orig_tick || note.key != orig_key as u8 {
                        // 移动不改变长度：恢复 start 时按当前 length 平移 end
                        // （forward 保证 end 跟随 start 平移，length 不变式成立）
                        let length = note.end_tick.saturating_sub(note.start_tick).max(1);
                        note.start_tick = super::super::accessors::f32_to_tick(orig_tick);
                        note.end_tick = note.start_tick.saturating_add(length);
                        note.key = orig_key as u8;
                        modified += 1;
                        modified_indices.push((track_id, idx));
                        applied.push(idx);
                    }
                } else {
                    let dt = op.delta_tick;
                    let dk = op.delta_key as i32;
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
                        modified_indices.push((track_id, idx));
                        applied.push(idx);
                    }
                }
            }

            // 阶段 3：恢复「按 start_tick 升序」不变式（二分查询依赖，
            // 破坏后渲染/命中会漏检音符）；重排轨的区间事件失效（见下）。
            if track.restore_sorted(&applied) {
                reordered_tracks.insert(track_id);
            }
        }

        if modified > 0 {
            let current_track = self.current_track;
            if reordered_tracks.contains(&current_track) {
                // 顺序已变：主轨区间事件按旧索引失效 → 全量重建
                // （渲染消费者遇 dirty 会丢弃积压事件，见 note_update.rs）
                self.note_delta_dirty = true;
            }
            // 仅当前轨的修改可用主轨段内 UpdateRange：事件队列无 track 维度，
            // 非当前轨的区间事件会被误应用到当前轨段（错误音符）。
            let current_only: Vec<(usize, usize)> = modified_indices
                .iter()
                .filter(|&&(t, _)| t == current_track && !reordered_tracks.contains(&t))
                .copied()
                .collect();
            self.push_move_update_ranges(&current_only);
            // 记录所有受影响音轨：若全部是当前音轨（洋葱皮不显示），
            // stream_onion_skin_instances 可豁免全量重建上传。
            let dirty_tracks: HashSet<usize> = ops.iter().map(|op| op.track_id as usize).collect();
            self.mark_track_notes_changed_for(Some(dirty_tracks));
        }
        modified
    }

    /// 将 `(track_id, index)` 修改集合按轨合并连续区间并推送 `UpdateRange` 事件。
    fn push_move_update_ranges(&mut self, modified_indices: &[(usize, usize)]) {
        let mut by_track: HashMap<usize, Vec<usize>> = HashMap::new();
        for &(track_id, idx) in modified_indices {
            by_track.entry(track_id).or_default().push(idx);
        }
        for (track_id, mut idxs) in by_track {
            idxs.sort_unstable();
            idxs.dedup();
            let mut start = idxs[0];
            let mut prev = idxs[0];
            let mut ranges: Vec<(usize, usize)> = Vec::new();
            for &i in &idxs[1..] {
                if i == prev + 1 {
                    prev = i;
                    continue;
                }
                ranges.push((start, prev));
                start = i;
                prev = i;
            }
            ranges.push((start, prev));
            for (s, e) in ranges {
                if let Some(notes) = self
                    .document
                    .as_ref()
                    .and_then(|doc| doc.track_notes(track_id).get_range(s..=e))
                {
                    let mapped: Vec<Note> = notes
                        .iter()
                        .copied()
                        .map(super::super::accessors::event_to_note)
                        .collect();
                    if !mapped.is_empty() {
                        self.note_delta_events.push(
                            crate::editor_state::editor_data::NoteDeltaEvent::UpdateRange {
                                start_index: s,
                                notes: mapped,
                            },
                        );
                    }
                }
            }
        }
    }

    /// 当前 view 下可用于 clamp key 的最大 key 索引
    ///
    /// EditorData 本身不持有 view，默认用 255（MIDI 最大 key）。
    /// UI 层调用 `apply_move_ops` 时应传入实际 `visible_key_count - 1`。
    pub(crate) fn max_key_for_move_op(&self) -> u16 {
        255
    }

    /// 从 DragState 构造 MoveOp 列表（按连续区间拆分，捕获全局唯一 id）。
    ///
    /// **优化**：`selected_indices()` 已按索引升序返回，无需 sort。
    /// 每段同时捕获 id 与原始 tick/key：id 供 undo/redo 精确重定位，
    /// 原始位置供 clamp 场景精确还原。
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

        // 直接遍历 document 当前轨提取 id 与原始 tick/key（单一权威源）
        let track_notes = self.current_track_notes();
        let make_op = |start: usize, end: usize, seq: u16| {
            let mut ids = Vec::with_capacity(end - start + 1);
            let mut ticks = Vec::with_capacity(end - start + 1);
            let mut keys = Vec::with_capacity(end - start + 1);
            if let Some(slice) = track_notes.get_range(start..=end) {
                for note in slice {
                    ids.push(note.id);
                    ticks.push(note.start_tick as f32);
                    keys.push(note.key as u16);
                }
            }
            MoveOp {
                track_id,
                ids,
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
        // 空段（索引越界防御）不进入历史——无可定位的 id 即无操作
        ops.retain(|op| !op.ids.is_empty());
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
