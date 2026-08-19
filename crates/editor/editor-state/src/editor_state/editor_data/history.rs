//! Undo/Redo 历史记录操作
//!
//! 2026-08 单一权威源改造：音符快照为 `Arc<Vec<NoteEvent>>`（从 document 轨道
//! 克隆，COW 共享）。恢复时经 `replace_track_notes` 写回 document。
//! `automation_lanes` 仍为 `Vec<Arc<AutomationLane>>`，编辑路径经 `Arc::make_mut` 写时复制。

use std::collections::HashSet;
use std::sync::Arc;

use super::EditorData;
use lumino_note_core::history::{CreateOp, EditorSnapshot, HistoryEntry, MoveOp, OpKind};

impl EditorData {
    // ── 快照构造 ─────────────────────────────────────────────

    /// 构造当前状态的 EditorSnapshot（不带元数据）
    ///
    /// `ChunkedList::clone` 为 O(块数) 浅拷贝（块 Arc COW），快照与 document
    /// 物理共享未修改块——1600W 音符工程快照不再复制整轨数据。
    fn make_snapshot(&self) -> EditorSnapshot {
        self.make_snapshot_for_track(self.current_track)
    }

    /// 构造指定音轨当前状态的 EditorSnapshot（不带元数据）
    ///
    /// 用于 undo/redo 时生成 redo/undo 快照：当用户切轨后 undo 另一条音轨的编辑，
    /// redo 快照必须记录**被 undo 的音轨**而非当前视图音轨，否则 redo 无法恢复。
    fn make_snapshot_for_track(&self, track_id: usize) -> EditorSnapshot {
        let notes = Arc::new(self.track_notes(track_id).clone());
        EditorSnapshot {
            notes,
            current_track: track_id,
            automation_lanes: self.automation_lanes.clone(),
            time_signatures: Some(self.time_signatures.clone()),
            tempo_points: Some(self.tempo_points.clone()),
            ..EditorSnapshot::new(
                Arc::new(lumino_midi_model::ChunkedList::new()),
                0,
                Vec::new(),
            )
        }
    }

    /// 从历史条目推断其影响的音轨（用于生成正确的 redo/undo 快照）。
    ///
    /// Snapshot：current_track 即被快照的音轨；
    /// Operation / Create：取第一个 op 的 track_id（常规单轨操作）。
    fn affected_track_of_history_entry(
        entry: &lumino_note_core::history::HistoryEntry,
    ) -> Option<usize> {
        match entry {
            lumino_note_core::history::HistoryEntry::Snapshot(s) => Some(s.current_track),
            lumino_note_core::history::HistoryEntry::Operation(op) => {
                op.ops.first().map(|o| o.track_id as usize)
            }
            lumino_note_core::history::HistoryEntry::Create(entry) => {
                entry.ops.first().map(|o| o.track_id as usize)
            }
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
        let track = self
            .history
            .undo_back()
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let current = self.make_snapshot_for_track(track);
        if let Some(entry) = self.history.undo(current) {
            self.apply_history_entry(entry, true);
            true
        } else {
            false
        }
    }

    /// 重做上一步撤销的操作（单步，不跨 chain）
    pub fn redo(&mut self) -> bool {
        let track = self
            .history
            .redo_back()
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let current = self.make_snapshot_for_track(track);
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
        let track = self
            .history
            .undo_back()
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let current = self.make_snapshot_for_track(track);
        if let Some(entry) = self.history.undo_logical(current) {
            self.apply_history_entry(entry, true);
            true
        } else {
            false
        }
    }

    /// 逻辑重做：跨同 group_id / parent_group_id 链一次性重做
    pub fn redo_logical(&mut self) -> bool {
        let track = self
            .history
            .redo_back()
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let current = self.make_snapshot_for_track(track);
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
            HistoryEntry::Snapshot(s) => {
                let track = s.current_track;
                self.apply_snapshot(*s);
                self.mark_tracks_changed_after_history(HashSet::from([track]));
            }
            HistoryEntry::Operation(op) => {
                // 对 OperationEntry：undo 时传入 inverse=true 按原始位置恢复；
                // redo 时 inverse=false 按 delta 前进。
                let affected: HashSet<usize> = op.ops.iter().map(|o| o.track_id as usize).collect();
                let _ = self.apply_move_ops(&op.ops, inverse, self.max_key_for_move_op());
                self.mark_tracks_changed_after_history(affected);
            }
            HistoryEntry::Create(entry) => {
                let affected: HashSet<usize> =
                    entry.ops.iter().map(|o| o.track_id as usize).collect();
                let _ = self.apply_create_ops(&entry.ops, inverse);
                self.mark_tracks_changed_after_history(affected);
            }
        }
    }

    /// 撤销/重做后标记受影响的音轨并清理过期增量事件。
    ///
    /// 与常规编辑不同，undo/redo 可能作用于非当前视图音轨。本方法：
    /// - 精确记录受影响的音轨集合，供洋葱皮层走 `TrackDelta` 增量同步。
    /// - 若当前音轨也在受影响集合内，由于 undo/redo 入口未记录主音轨段内
    ///   增量事件，必须走 `note_delta_dirty` 全量兜底重建。
    /// - 清空可能残留的旧 `note_delta_events`，防止其在新 `current_track` 下
    ///   被误应用到错误音轨。
    fn mark_tracks_changed_after_history(&mut self, affected_tracks: HashSet<usize>) {
        self.note_delta_events.clear();
        self.onion_dirty_tracks = Some(affected_tracks.clone());
        self.track_notes_gen = self.track_notes_gen.wrapping_add(1);
        self.note_delta_dirty = affected_tracks.contains(&self.current_track);
    }

    /// 应用快照到当前状态（undo / redo 后调用）
    fn apply_snapshot(&mut self, snapshot: EditorSnapshot) {
        // 音符快照写回 document（整轨替换，单一权威源）。
        // O(块数) 浅拷贝：直接共享快照块 Arc，不复制音符数据。
        //
        // 注意：快照的 current_track 仅用于定位该快照音符属于哪条音轨，
        // **不得恢复为当前视图音轨**。undo/redo 只应恢复数据，不应改变用户
        // 当前正在查看的音轨；否则切轨后按 Ctrl+Z 会跳回上一条被编辑的音轨。
        self.replace_track_notes_chunked(snapshot.current_track, snapshot.notes.as_ref());
        self.automation_lanes = snapshot.automation_lanes.clone();
        if let Some(v) = snapshot.time_signatures {
            // 经统一入口恢复，保证 document.time_signatures 同步（保存链路读到最新）
            self.set_time_signatures(v);
        }
        if let Some(v) = snapshot.tempo_points {
            // 经统一入口恢复，保证 document.tempo_changes 同步（保存链路读到最新）
            self.set_tempo_points(v);
        }
    }

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
}

mod ops;

#[cfg(test)]
mod tests;
