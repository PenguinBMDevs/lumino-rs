impl super::Editor {
    /// 切换到指定音轨（无 MIDI 文件时使用）
    pub fn switch_to_track(&mut self, track_idx: usize) {
        if self.editor_state.data.current_track == track_idx {
            return;
        }

        tracing::debug!(
            "Editor: switching from track {} to {}",
            self.editor_state.data.current_track,
            track_idx
        );

        // 保存当前音轨的音符
        self.editor_state.data.track_notes.insert(
            self.editor_state.data.current_track,
            self.editor_state.data.notes.clone(),
        );

        tracing::debug!(
            "Editor: saved {} notes for track {}",
            self.editor_state.data.notes.len(),
            self.editor_state.data.current_track
        );

        // 切换到新音轨
        self.editor_state.data.current_track = track_idx;

        // 加载新音轨的音符：优先从 track_notes 缓存读，缓存未命中则从 document 懒加载
        self.editor_state.data.notes =
            if let Some(cached) = self.editor_state.data.track_notes.get(&track_idx).cloned() {
                cached
            } else if let Some(doc) = self.editor_state.data.document.as_ref() {
                let raw = doc.get_track_notes(track_idx as u16);
                let mut notes = im::Vector::new();
                for (tick, key, length, velocity, channel) in raw {
                    notes.push_back(crate::note::Note::from_raw(
                        tick, key as u16, length, velocity, channel,
                    ));
                }
                self.editor_state
                    .data
                    .track_notes
                    .insert(track_idx, notes.clone());
                notes
            } else {
                im::Vector::new()
            };
        tracing::debug!(
            "Editor: loaded {} notes for track {}",
            self.editor_state.data.notes.len(),
            track_idx
        );

        // 切换音轨时清除选中状态（通过 editor_state）
        self.editor_state.interaction.selected_notes.clear();
        self.editor_state.interaction.hover_state = None;
        self.editor_state.interaction.edit_state = super::EditState::Idle;
        // 标记音符数据已变化，触发空间索引重建和渲染更新
        self.mark_notes_changed();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.editor_state.data.current_track
    }
}
