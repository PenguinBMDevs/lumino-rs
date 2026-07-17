//! Undo/Redo 历史记录操作
//!
//! `EditorData` 与 `EditorSnapshot` 均使用 `Vec<Arc<AutomationLane>>`，
//! 快照克隆为 O(lane 数) 的 Arc 指针拷贝，未修改的 lane 物理共享。
//! 编辑路径通过 `Arc::make_mut` 写时复制（见 editor_data/automation.rs）。

use super::EditorData;
use crate::history::EditorSnapshot;

impl EditorData {
    /// 将当前状态快照推入历史记录（O(lane 数) Arc clone，真共享）
    pub fn push_history(&mut self) {
        self.history.push(EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            self.automation_lanes.clone(),
        ));
    }

    /// 撤销上一次操作
    pub fn undo(&mut self) -> bool {
        let current = EditorSnapshot::new(
            self.notes.clone(),
            self.current_track,
            self.automation_lanes.clone(),
        );
        if let Some(snapshot) = self.history.undo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.automation_lanes = snapshot.automation_lanes.clone();
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
            self.automation_lanes.clone(),
        );
        if let Some(snapshot) = self.history.redo(current) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.automation_lanes = snapshot.automation_lanes.clone();
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
