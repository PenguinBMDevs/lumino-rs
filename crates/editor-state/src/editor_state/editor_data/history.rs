//! Undo/Redo 历史记录操作
//!
//! 2026-08 单一权威源改造：音符快照为 `Arc<Vec<NoteEvent>>`（从 document 轨道
//! 克隆，COW 共享）。恢复时经 `replace_track_notes` 写回 document。
//! `automation_lanes` 仍为 `Vec<Arc<AutomationLane>>`，编辑路径经 `Arc::make_mut` 写时复制。

use std::sync::Arc;

use super::EditorData;
use crate::DragState;
use lumino_note_core::history::{CreateOp, EditorSnapshot, HistoryEntry, MoveOp, OpKind};

impl EditorData {
    // ── 快照构造 ─────────────────────────────────────────────

    /// 构造当前状态的 EditorSnapshot（不带元数据）
    ///
    /// `ChunkedList::clone` 为 O(块数) 浅拷贝（块 Arc COW），快照与 document
    /// 物理共享未修改块——1600W 音符工程快照不再复制整轨数据。
    fn make_snapshot(&self) -> EditorSnapshot {
        let notes = Arc::new(self.current_track_notes().clone());
        EditorSnapshot {
            notes,
            current_track: self.current_track,
            automation_lanes: self.automation_lanes.clone(),
            time_signatures: Some(self.time_signatures.clone()),
            key_signatures: Some(self.key_signatures.clone()),
            markers: Some(self.markers.clone()),
            lyrics: Some(self.lyrics.clone()),
            chords: Some(self.chords.clone()),
            program_changes: Some(self.program_changes.clone()),
            tempo_points: Some(self.tempo_points.clone()),
            ..EditorSnapshot::new(
                Arc::new(lumino_midi_model::ChunkedList::new()),
                0,
                Vec::new(),
            )
        }
    }

    // ── 向后兼容的 push / undo / redo ───────────────────────

    /// 将当前状态快照推入历史记录（O(lane 数) Arc clone，真共享）
    ///
    /// 向后兼容版本：op_kind = Other，每个 push 独立 group。
    /// **新代码应使用 `push_history_with_op_kind` 或 `push_history_mergeable`**
    /// 以获得逻辑撤销链 / 合并窗口能力。
    pub fn push_history(&mut self) {
        self.history.push(self.make_snapshot());
    }

    /// 推入带 op_kind 的快照（不合并，但分配独立 group_id）
    ///
    /// 适用：NoteMove / NoteDelete / NoteTransform / VelocityEdit / AutomationEdit / Recording
    /// 这些操作不走合并窗口，但需要 group_id 以支持未来扩展（如批量操作的逻辑分组）。
    pub fn push_history_with_op_kind(&mut self, op_kind: OpKind) {
        self.history
            .push_with_op_kind(self.make_snapshot(), op_kind);
    }

    /// 推入可合并的快照（仅 `OpKind::NoteCreate` 等可合并类型）
    ///
    /// 适用：Pencil 绘制连续放置音符。
    /// 合并规则：栈顶 op_kind 相同 + 在合并窗口内 + 未超 entry 上限 → 合并。
    /// 返回 `true` 表示合并到上一条，`false` 表示新增/分割。
    pub fn push_history_mergeable(&mut self, op_kind: OpKind) -> bool {
        self.history.push_mergeable(self.make_snapshot(), op_kind)
    }

    /// 推入 NoteMove 操作日志
    ///
    /// 拖动提交路径使用：用轻量 MoveOp 替代完整快照。
    pub fn push_move_op(&mut self, ops: Vec<MoveOp>) -> u64 {
        self.history.push_move_op(ops)
    }

    /// 推入音符创建操作日志（NoteCreate 增量、极简化）
    ///
    /// 铅笔绘制路径使用：每 op 仅 20 字节，替代整轨快照克隆——
    /// 1600W 音符工程在合并窗口内连续绘制不再复制音符数据。
    /// 返回 `true` 表示合并到上一条（300ms 窗口内）。
    pub fn push_note_create(&mut self, ops: Vec<CreateOp>) -> bool {
        self.history.push_note_create(ops)
    }

    // ── 单步 undo / redo ────────────────────────────────────

    /// 撤销上一步操作（单步，不跨 chain）
    pub fn undo(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.undo(current) {
            self.apply_history_entry(entry, true);
            true
        } else {
            false
        }
    }

    /// 重做上一步撤销的操作（单步，不跨 chain）
    pub fn redo(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.redo(current) {
            self.apply_history_entry(entry, false);
            true
        } else {
            false
        }
    }

    // ── 逻辑 undo / redo（跨 chain）────────────────────────

    /// 逻辑撤销：跨同 group_id / parent_group_id 链一次性撤销
    ///
    /// 适用：用户感知的"撤销刚才那一波放置"——即使分割为多个 group，
    /// 也应一次性回退整个逻辑操作。
    pub fn undo_logical(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.undo_logical(current) {
            self.apply_history_entry(entry, true);
            true
        } else {
            false
        }
    }

    /// 逻辑重做：跨同 group_id / parent_group_id 链一次性重做
    pub fn redo_logical(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.redo_logical(current) {
            self.apply_history_entry(entry, false);
            true
        } else {
            false
        }
    }

    // ── 历史记录条目应用 ─────────────────────────────────────

    /// 根据 HistoryEntry 类型应用撤销/重做
    fn apply_history_entry(&mut self, entry: HistoryEntry, inverse: bool) {
        match entry {
            HistoryEntry::Snapshot(s) => self.apply_snapshot(*s),
            HistoryEntry::Operation(op) => {
                // 对 OperationEntry：undo 时传入 inverse=true 按原始位置恢复；
                // redo 时 inverse=false 按 delta 前进。
                let _ = self.apply_move_ops(&op.ops, inverse, self.max_key_for_move_op());
            }
            HistoryEntry::Create(entry) => {
                let _ = self.apply_create_ops(&entry.ops, inverse);
            }
        }
    }

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

    /// 应用快照到当前状态（undo / redo 后调用）
    fn apply_snapshot(&mut self, snapshot: EditorSnapshot) {
        // 音符快照写回 document（整轨替换，单一权威源）。
        // O(块数) 浅拷贝：直接共享快照块 Arc，不复制音符数据。
        self.replace_track_notes_chunked(snapshot.current_track, snapshot.notes.as_ref());
        self.current_track = snapshot.current_track;
        self.automation_lanes = snapshot.automation_lanes.clone();
        if let Some(v) = snapshot.time_signatures {
            self.time_signatures = v;
        }
        if let Some(v) = snapshot.key_signatures {
            self.key_signatures = v;
        }
        if let Some(v) = snapshot.markers {
            self.markers = v;
        }
        if let Some(v) = snapshot.lyrics {
            self.lyrics = v;
        }
        if let Some(v) = snapshot.chords {
            self.chords = v;
        }
        if let Some(v) = snapshot.program_changes {
            self.program_changes = v;
        }
        if let Some(v) = snapshot.tempo_points {
            self.tempo_points = v;
        }
    }

    /// 当前 view 下可用于 clamp key 的最大 key 索引
    ///
    /// EditorData 本身不持有 view，默认用 255（MIDI 最大 key）。
    /// UI 层调用 `apply_move_ops` 时应传入实际 `visible_key_count - 1`。
    fn max_key_for_move_op(&self) -> u16 {
        255
    }

    // ── 辅助方法 ────────────────────────────────────────────

    /// 是否可以撤销
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// 是否可以重做
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 丢弃最近一次 undo 条目（push 后发现无实际变更时调用，不触碰 redo 栈）
    pub fn discard_last_history(&mut self) {
        self.history.discard_last();
    }

    // ── MoveOp 应用与构造 ────────────────────────────────────

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
                        note.start_tick = super::accessors::f32_to_tick(orig_tick);
                        note.end_tick = note
                            .end_tick
                            .saturating_sub(0)
                            .max(note.start_tick.saturating_add(1));
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
                            note.end_tick = note.end_tick.max(new_tick.saturating_add(1));
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
pub trait SaturatingInto<T> {
    /// 饱和转换
    fn saturating_into(self) -> T;
}

impl SaturatingInto<i32> for i64 {
    fn saturating_into(self) -> i32 {
        self.clamp(i32::MIN as i64, i32::MAX as i64) as i32
    }
}

#[cfg(test)]
mod tests;
