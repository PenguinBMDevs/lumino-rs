//! 单步 / 逻辑 undo / redo 与历史栈状态查询
//!
//! 由 `history.rs` 拆分而来。

use super::*;

impl EditorData {
    /// 取走自上一笔入栈以来积累的按值变更。
    fn take_history_pending(&mut self) -> (Vec<SnapshotNoteChange>, bool) {
        (
            std::mem::take(&mut self.history_pending_changes),
            std::mem::take(&mut self.history_pending_full),
        )
    }

    /// 解析本次回放应使用的变更：当前未入栈 pending 优先，否则用条目自带。
    fn resolve_history_changes(
        &self,
        pending: &(Vec<SnapshotNoteChange>, bool),
        entry: Option<&HistoryEntry>,
    ) -> (Vec<SnapshotNoteChange>, bool) {
        if !pending.0.is_empty() || pending.1 {
            return (pending.0.clone(), pending.1);
        }
        match entry {
            Some(HistoryEntry::Snapshot(snapshot)) => {
                (snapshot.note_changes.clone(), snapshot.note_changes_full)
            }
            _ => (Vec::new(), false),
        }
    }

    // ── 单步 undo / redo ────────────────────────────────────

    /// 撤销上一步操作（单步，不跨 chain）
    pub fn undo(&mut self) -> bool {
        let pending = self.take_history_pending();
        let target = self.history.undo_back();
        let track = target
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let (changes, full) = self.resolve_history_changes(&pending, target);
        let mut current = self.make_snapshot_for_track(track);
        current.note_changes = changes.clone();
        current.note_changes_full = full;
        if let Some(entry) = self.history.undo(current) {
            self.apply_history_entry(entry, true, changes, full);
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// 重做上一步撤销的操作（单步，不跨 chain）
    pub fn redo(&mut self) -> bool {
        let pending = self.take_history_pending();
        let target = self.history.redo_back();
        let track = target
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let (changes, full) = self.resolve_history_changes(&pending, target);
        let mut current = self.make_snapshot_for_track(track);
        current.note_changes = changes.clone();
        current.note_changes_full = full;
        if let Some(entry) = self.history.redo(current) {
            self.apply_history_entry(entry, false, changes, full);
            self.modified = true;
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
        let pending = self.take_history_pending();
        let target = self.history.undo_back();
        let track = target
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let (changes, full) = self.resolve_history_changes(&pending, target);
        let mut current = self.make_snapshot_for_track(track);
        current.note_changes = changes.clone();
        current.note_changes_full = full;
        if let Some(entry) = self.history.undo_logical(current) {
            self.apply_history_entry(entry, true, changes, full);
            true
        } else {
            false
        }
    }

    /// 逻辑重做：跨同 group_id / parent_group_id 链一次性重做
    pub fn redo_logical(&mut self) -> bool {
        let pending = self.take_history_pending();
        let target = self.history.redo_back();
        let track = target
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let (changes, full) = self.resolve_history_changes(&pending, target);
        let mut current = self.make_snapshot_for_track(track);
        current.note_changes = changes.clone();
        current.note_changes_full = full;
        if let Some(entry) = self.history.redo_logical(current) {
            self.apply_history_entry(entry, false, changes, full);
            true
        } else {
            false
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
