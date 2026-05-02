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
        self.track_note_indices
            .borrow_mut()
            .remove(&self.current_track);

        tracing::debug!(
            "Editor: saved {} notes for track {}",
            self.notes.len(),
            self.current_track
        );

        // 切换到新音轨
        self.current_track = track_idx;

        // 加载新音轨的音符：优先从 track_notes 缓存读，缓存未命中则从 document 懒加载
        self.notes = if let Some(cached) = self.track_notes.get(&track_idx).cloned() {
            cached
        } else if let Some(doc) = self.document.as_ref() {
            let raw = doc.get_track_notes(track_idx as u16);
            let mut notes = im::Vector::new();
            for (tick, key, length, velocity, channel) in raw {
                notes.push_back(
                    crate::editor::note::Note::new(tick, key as u16, length)
                        .with_velocity(velocity)
                        .with_channel(channel),
                );
            }
            self.track_notes.insert(track_idx, notes.clone());
            notes
        } else {
            im::Vector::new()
        };
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
