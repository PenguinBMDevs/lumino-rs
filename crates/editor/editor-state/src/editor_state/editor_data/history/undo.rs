//! 单步 / 逻辑 undo / redo 与历史栈状态查询
//!
//! 由 `history.rs` 拆分而来。

use super::*;

/// 历史回放方向。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryDir {
    Undo,
    Redo,
}

impl EditorData {
    /// 取走自上一笔入栈以来积累的按值变更。
    fn take_history_pending(&mut self) -> (Vec<SnapshotNoteChange>, Vec<(u32, u32)>, bool) {
        (
            std::mem::take(&mut self.history_pending_changes),
            std::mem::take(&mut self.history_pending_ranges),
            std::mem::take(&mut self.history_pending_full),
        )
    }

    /// 解析本次回放应使用的变更：当前未入栈 pending 优先，否则用条目自带。
    fn resolve_history_changes(
        &self,
        pending: &(Vec<SnapshotNoteChange>, Vec<(u32, u32)>, bool),
        entry: Option<&HistoryEntry>,
    ) -> (Vec<SnapshotNoteChange>, Vec<(u32, u32)>, bool) {
        if !pending.0.is_empty() || !pending.1.is_empty() || pending.2 {
            return (pending.0.clone(), pending.1.clone(), pending.2);
        }
        match entry {
            Some(HistoryEntry::Snapshot(snapshot)) => (
                snapshot.note_changes.clone(),
                snapshot.dirty_ranges.clone(),
                snapshot.note_changes_full,
            ),
            _ => (Vec::new(), Vec::new(), false),
        }
    }

    // ── 单步 / 逻辑 undo / redo ─────────────────────────────

    /// 撤销/重做四个入口的统一骨架。
    ///
    /// `logical` 为 true 时不置 `modified`——逻辑撤销不把文档标记为脏是现状语义
    /// （单步 undo/redo 才置位），合并时必须保留。
    fn history_step(&mut self, dir: HistoryDir, logical: bool) -> bool {
        self.commit_history_pending();
        let pending = self.take_history_pending();
        let target = match dir {
            HistoryDir::Undo => self.history.undo_back(),
            HistoryDir::Redo => self.history.redo_back(),
        };
        let track = target
            .and_then(Self::affected_track_of_history_entry)
            .unwrap_or(self.current_track);
        let (changes, ranges, full) = self.resolve_history_changes(&pending, target);
        let mut current = self.make_snapshot_for_track(track);
        current.note_changes = changes.clone();
        current.dirty_ranges = ranges.clone();
        current.note_changes_full = full;
        let entry = match (dir, logical) {
            (HistoryDir::Undo, false) => self.history.undo(current),
            (HistoryDir::Redo, false) => self.history.redo(current),
            (HistoryDir::Undo, true) => self.history.undo_logical(current),
            (HistoryDir::Redo, true) => self.history.redo_logical(current),
        };
        if let Some(entry) = entry {
            self.apply_history_entry(entry, dir == HistoryDir::Undo, changes, ranges, full);
            if !logical {
                self.modified = true;
            }
            true
        } else {
            false
        }
    }

    /// 撤销上一步操作（单步，不跨 chain）
    pub fn undo(&mut self) -> bool {
        self.history_step(HistoryDir::Undo, false)
    }

    /// 重做上一步撤销的操作（单步，不跨 chain）
    pub fn redo(&mut self) -> bool {
        self.history_step(HistoryDir::Redo, false)
    }

    /// 逻辑撤销：跨同 group_id / parent_group_id 链一次性撤销
    ///
    /// 适用：用户感知的"撤销刚才那一波放置"——即使分割为多个 group，
    /// 也应一次性回退整个逻辑操作。
    pub fn undo_logical(&mut self) -> bool {
        self.history_step(HistoryDir::Undo, true)
    }

    /// 逻辑重做：跨同 group_id / parent_group_id 链一次性重做
    pub fn redo_logical(&mut self) -> bool {
        self.history_step(HistoryDir::Redo, true)
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
