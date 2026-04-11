impl super::Editor {
    /// 切换到指定音轨（无 MIDI 文件时使用）
    pub fn switch_to_track(&mut self, track_idx: usize) {
        if self.current_track == track_idx {
            return;
        }

        tracing::debug!(
            "Editor: switching from track {} to {}",
            self.current_track,
            track_idx
        );

        // 保存当前音轨的音符
        self.track_notes
            .insert(self.current_track, self.notes.clone());
        self.track_note_indices.borrow_mut().remove(&self.current_track);
        
        tracing::debug!(
            "Editor: saved {} notes for track {}",
            self.notes.len(),
            self.current_track
        );

        // 切换到新音轨
        self.current_track = track_idx;

        // 加载新音轨的音符
        self.notes = self
            .track_notes
            .get(&track_idx)
            .cloned()
            .unwrap_or_default();
        tracing::debug!(
            "Editor: loaded {} notes for track {}",
            self.notes.len(),
            track_idx
        );

        // 标记音符数据已变化，触发空间索引重建和渲染更新
        self.mark_notes_changed();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.current_track
    }
}
