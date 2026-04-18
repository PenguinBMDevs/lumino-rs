use crate::editor::Editor;

impl Editor {
    /// Push current state to history
    pub fn push_history(&mut self) {
        let snapshot = super::history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::debug!(
            "推送历史记录: {} 个音符，音轨 {}",
            snapshot.notes.len(),
            snapshot.current_track
        );
        self.history.push(snapshot);
    }

    /// Undo the last action
    pub fn undo(&mut self) -> bool {
        let current_state =
            super::history::EditorSnapshot::new(self.notes.clone(), self.current_track);
        tracing::info!(
            "尝试撤销: 当前音符数 = {}, 可撤销 = {}",
            self.notes.len(),
            self.can_undo()
        );

        if let Some(snapshot) = self.history.undo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("撤销操作成功: {} 个音符", self.notes.len());
            true
        } else {
            tracing::info!("没有可撤销的操作");
            false
        }
    }

    /// Redo the last undone action
    pub fn redo(&mut self) -> bool {
        let current_state =
            super::history::EditorSnapshot::new(self.notes.clone(), self.current_track);

        if let Some(snapshot) = self.history.redo(current_state) {
            self.notes = snapshot.notes;
            self.current_track = snapshot.current_track;
            self.grid_cache.clear();
            self.mark_notes_changed();
            tracing::info!("重做操作成功");
            true
        } else {
            tracing::info!("没有可重做的操作");
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }
}
