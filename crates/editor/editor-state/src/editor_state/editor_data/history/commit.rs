//! 历史 pending 落栈：逐音符变更、脏 tick 区间、直接编辑索引区间。

use super::*;

impl EditorData {
    /// 把上一笔操作的按值变更/区间挂到栈顶快照，然后清空待挂队列。
    ///
    /// 调用点都在「新一笔操作入栈前」：此时上一笔已经结束，pending 正好是
    /// 它从栈顶快照到当前状态的完整差分。直接编辑没有 old/new 时，用
    /// `pending_direct` 索引区间与快照/当前文档差分出脏 tick 区间。
    pub(super) fn commit_history_pending(&mut self) {
        if self.history_pending_changes.is_empty()
            && self.history_pending_ranges.is_empty()
            && self.history_pending_direct.is_empty()
            && !self.history_pending_full
        {
            return;
        }
        let changes = std::mem::take(&mut self.history_pending_changes);
        let mut ranges = std::mem::take(&mut self.history_pending_ranges);
        let direct = std::mem::take(&mut self.history_pending_direct);
        let mut full = std::mem::take(&mut self.history_pending_full);
        if !direct.is_empty() && !full {
            let Some(document) = self.document.as_ref() else {
                full = true;
                self.history.attach_last_note_changes(changes, ranges, full);
                return;
            };
            let snapshot = match self.history.undo_back() {
                Some(HistoryEntry::Snapshot(snapshot)) => Some(snapshot.as_ref()),
                _ => None,
            };
            let track = snapshot.map_or(self.current_track, |snapshot| snapshot.current_track);
            let (same, other): (Vec<_>, Vec<_>) =
                direct.into_iter().partition(|range| range.0 == track);
            let previous = snapshot.map(|snapshot| snapshot.notes.as_ref());
            ranges.extend(super::ranges::direct_ranges(
                previous,
                document.track_notes(track),
                &same,
            ));
            for (other_track, lo, hi) in other {
                ranges.extend(super::ranges::direct_ranges(
                    None,
                    document.track_notes(other_track),
                    &[(other_track, lo, hi)],
                ));
            }
        }
        self.history.attach_last_note_changes(changes, ranges, full);
    }

    /// 入栈创建日志前，先摘掉属于本次创建操作的 pending 插入。
    pub(super) fn commit_history_pending_for_create(&mut self, ops: &[CreateOp]) {
        let full = std::mem::take(&mut self.history_pending_full);
        let pending = std::mem::take(&mut self.history_pending_changes);
        let ranges = std::mem::take(&mut self.history_pending_ranges);
        self.history_pending_direct.clear();
        if full {
            self.history
                .attach_last_note_changes(Vec::new(), ranges, true);
            return;
        }
        let mut remaining = pending;
        for op in ops {
            if let Some(index) = remaining.iter().position(|change| {
                change.track == op.track_id as usize
                    && change.old.is_none()
                    && change.new == Some(op.note)
            }) {
                remaining.remove(index);
            }
        }
        self.history
            .attach_last_note_changes(remaining, ranges, false);
    }
}
