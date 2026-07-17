//! Undo/Redo 历史记录操作
//!
//! `EditorData` 用 `Vec<AutomationLane>`（编辑器本体频繁读写），
//! `EditorSnapshot` 用 `Vector<AutomationLane>`（快照层持久化共享）。
//! 转换开销仅在快照创建/恢复时发生，不影响编辑器正常渲染循环。

use super::EditorData;
use crate::history::EditorSnapshot;
use im::Vector;

impl EditorData {
    /// 将当前状态快照推入历史记录
    pub fn push_history(&mut self) {
        self.history.push(EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            Vector::from(self.automation_lanes.clone()),
        ));
    }

    /// 撤销上一次操作
    pub fn undo(&mut self) -> bool {
        let current = EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            Vector::from(self.automation_lanes.clone()),
        );
        if let Some(snapshot) = self.history.undo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.automation_lanes = snapshot.automation_lanes.into_iter().collect();
            true
        } else {
            false
        }
    }

    /// 重做上一次撤销的操作
    pub fn redo(&mut self) -> bool {
        let current = EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            Vector::from(self.automation_lanes.clone()),
        );
        if let Some(snapshot) = self.history.redo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.automation_lanes = snapshot.automation_lanes.into_iter().collect();
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
}
