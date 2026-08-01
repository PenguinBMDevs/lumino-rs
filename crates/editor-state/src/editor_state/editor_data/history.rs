//! Undo/Redo 历史记录操作
//!
//! `EditorData` 与 `EditorSnapshot` 均使用 `Vec<Arc<AutomationLane>>`，
//! 快照克隆为 O(lane 数) 的 Arc 指针拷贝，未修改的 lane 物理共享。
//! 编辑路径通过 `Arc::make_mut` 写时复制（见 editor_data/automation.rs）。

use super::EditorData;
use crate::DragState;
use lumino_note_core::history::{EditorSnapshot, HistoryEntry, MoveOp, OpKind};

impl EditorData {
    // ── 快照构造 ─────────────────────────────────────────────

    /// 构造当前状态的 EditorSnapshot（不带元数据）
    fn make_snapshot(&self) -> EditorSnapshot {
        EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            self.automation_lanes.clone(),
        )
    }

    // ── 向后兼容的 push / undo / redo ───────────────────────

    /// 如果 `note_store_dirty`，先同步 NoteStore 到 `notes`，确保快照捕获最新状态
    fn sync_from_store_if_dirty(&mut self) {
        if self.note_store_dirty {
            self.sync_notes_from_store();
            self.note_store_dirty = false;
        }
    }

    /// 将当前状态快照推入历史记录（O(lane 数) Arc clone，真共享）
    ///
    /// 向后兼容版本：op_kind = Other，每个 push 独立 group。
    /// **新代码应使用 `push_history_with_op_kind` 或 `push_history_mergeable`**
    /// 以获得逻辑撤销链 / 合并窗口能力。
    pub fn push_history(&mut self) {
        self.sync_from_store_if_dirty();
        self.history.push(self.make_snapshot());
    }

    /// 推入带 op_kind 的快照（不合并，但分配独立 group_id）
    ///
    /// 适用：NoteMove / NoteDelete / NoteTransform / VelocityEdit / AutomationEdit / Recording
    /// 这些操作不走合并窗口，但需要 group_id 以支持未来扩展（如批量操作的逻辑分组）。
    pub fn push_history_with_op_kind(&mut self, op_kind: OpKind) {
        self.sync_from_store_if_dirty();
        self.history
            .push_with_op_kind(self.make_snapshot(), op_kind);
    }

    /// 推入可合并的快照（仅 `OpKind::NoteCreate` 等可合并类型）
    ///
    /// 适用：Pencil 绘制连续放置音符。
    /// 合并规则：栈顶 op_kind 相同 + 在合并窗口内 + 未超 entry 上限 → 合并。
    /// 返回 `true` 表示合并到上一条，`false` 表示新增/分割。
    pub fn push_history_mergeable(&mut self, op_kind: OpKind) -> bool {
        self.sync_from_store_if_dirty();
        self.history.push_mergeable(self.make_snapshot(), op_kind)
    }

    /// 推入 NoteMove 操作日志
    ///
    /// 拖动提交路径使用：用轻量 MoveOp 替代完整快照。
    pub fn push_move_op(&mut self, ops: Vec<MoveOp>) -> u64 {
        self.history.push_move_op(ops)
    }

    // ── 单步 undo / redo ────────────────────────────────────

    /// 撤销上一步操作（单步，不跨 chain）
    ///
    /// 恢复快照后同步 `note_store`，确保 NoteStore 热路径不因 `notes` 回退而不同步。
    pub fn undo(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.undo(current) {
            self.apply_history_entry(entry, true);
            // 恢复快照后同步 note_store（快照只存 notes，不存 note_store）
            self.sync_note_store();
            true
        } else {
            false
        }
    }

    /// 重做上一步撤销的操作（单步，不跨 chain）
    ///
    /// 恢复快照后同步 `note_store`，确保 NoteStore 热路径不因 `notes` 回退而不同步。
    pub fn redo(&mut self) -> bool {
        let current = self.make_snapshot();
        if let Some(entry) = self.history.redo(current) {
            self.apply_history_entry(entry, false);
            // 恢复快照后同步 note_store
            self.sync_note_store();
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
            self.sync_note_store();
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
            self.sync_note_store();
            true
        } else {
            false
        }
    }

    // ── 历史记录条目应用 ─────────────────────────────────────

    /// 根据 HistoryEntry 类型应用撤销/重做
    fn apply_history_entry(&mut self, entry: HistoryEntry, inverse: bool) {
        match entry {
            HistoryEntry::Snapshot(s) => self.apply_snapshot(s),
            HistoryEntry::Operation(op) => {
                // 对 OperationEntry：undo 时传入 inverse=true 按原始位置恢复；
                // redo 时 inverse=false 按 delta 前进。
                let _ = self.apply_move_ops(&op.ops, inverse, self.max_key_for_move_op());
            }
        }
    }

    /// 应用快照到当前状态（undo / redo 后调用）
    fn apply_snapshot(&mut self, snapshot: EditorSnapshot) {
        self.notes = snapshot.notes;
        self.current_track = snapshot.current_track;
        self.automation_lanes = snapshot.automation_lanes.clone();
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

    /// 应用 MoveOp 列表到 notes 和对应 track_notes。
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

            // 确保 track_notes 中存在该 track 的缓存
            if !self.track_notes.contains_key(&track_id) && !self.notes.is_empty() {
                self.track_notes.insert(track_id, self.notes.clone());
            }

            if inverse {
                // 按原始位置恢复，确保 clamp 后的音符也能精确还原
                for (idx, i) in (start..end).enumerate() {
                    if idx >= op.original_ticks.len() || idx >= op.original_keys.len() {
                        break;
                    }
                    let orig_tick = op.original_ticks[idx];
                    let orig_key = op.original_keys[idx];
                    if let Some(note) = self.notes.get_mut(i)
                        && ((note.tick - orig_tick).abs() > f32::EPSILON || note.key != orig_key)
                    {
                        note.tick = orig_tick;
                        note.key = orig_key;
                        modified += 1;
                    }
                    if let Some(track_notes) = self.track_notes.get_mut(&track_id)
                        && let Some(note) = track_notes.get_mut(i)
                    {
                        note.tick = orig_tick;
                        note.key = orig_key;
                    }
                }
            } else {
                let dt = op.delta_tick as f32;
                let dk = op.delta_key as i32;
                for i in start..end {
                    if let Some(note) = self.notes.get_mut(i) {
                        let new_tick = (note.tick + dt).max(0.0);
                        let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u16;
                        if (note.tick - new_tick).abs() > f32::EPSILON || note.key != new_key {
                            note.tick = new_tick;
                            note.key = new_key;
                            modified += 1;
                        }
                    }
                    if let Some(track_notes) = self.track_notes.get_mut(&track_id)
                        && let Some(note) = track_notes.get_mut(i)
                    {
                        let new_tick = (note.tick + dt).max(0.0);
                        let new_key = (note.key as i32 + dk).clamp(0, max_key as i32) as u16;
                        note.tick = new_tick;
                        note.key = new_key;
                    }
                }
            }
        }

        if modified > 0 {
            self.mark_track_notes_changed();
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

        // NoteStore 启用时用 range_ticks_keys（顺序扫描，O(N) 一次二分查找），
        // 否则用 notes.get（clone Note）
        let make_op = |start: usize, end: usize, seq: u16| {
            let (ticks, keys): (Vec<f32>, Vec<u16>) = if self.note_store_enabled {
                self.note_store
                    .range_ticks_keys(start, end + 1)
                    .into_iter()
                    .unzip()
            } else {
                (start..=end)
                    .filter_map(|idx| self.notes.get(idx).map(|note| (note.tick, note.key)))
                    .unzip()
            };
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
