impl super::Editor {
    /// 切换到指定音轨（无 MIDI 文件时使用）
    pub fn switch_to_track(&mut self, track_idx: usize) {
        if self.current_track == track_idx {
            return;
        }

        let old_track = self.current_track;

        tracing::debug!(
            "Editor: switching from track {} to {}",
            old_track,
            track_idx
        );

        // 保存当前音轨的音符
        self.track_notes.insert(old_track, self.notes.clone());
        self.track_note_indices.borrow_mut().remove(&old_track);

        // 标记旧音轨的洋葱皮缓存为脏（可能在当前会话中被编辑过）
        self.onion_skin_dirty.borrow_mut().insert(old_track);

        tracing::debug!(
            "Editor: saved {} notes for track {}",
            self.notes.len(),
            old_track
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

        // 如果新音轨有缓存数据，且是新音轨，预先生成缓存（用于在旧音轨作为洋葱皮显示）
        if track_idx != old_track {
            self.onion_skin_dirty.borrow_mut().insert(track_idx);
        }

        // 标记音符数据已变化，触发空间索引重建和渲染更新
        self.mark_notes_changed();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.current_track
    }
}
