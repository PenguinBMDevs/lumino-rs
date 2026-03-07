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
        if !self.notes.is_empty() {
            self.track_notes
                .insert(self.current_track, self.notes.clone());
            tracing::debug!(
                "Editor: saved {} notes for track {}",
                self.notes.len(),
                self.current_track
            );
        }

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

        // 清除网格缓存以强制重绘
        self.grid_cache.clear();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.current_track
    }
}
