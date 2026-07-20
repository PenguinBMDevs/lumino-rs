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
        self.selection_clear();
        self.editor_state.interaction.hover_state = None;
        self.editor_state.interaction.edit_state = super::EditState::Idle;
        // 切轨只是替换当前显示的音符（data.notes 换成另一轨的数据），
        // 并非用户编辑。需要重建空间索引并失效渲染缓存，
        // 但不能设置 notes_changed，否则会被 handle_action 误判为脏音轨，
        // 导致高精度洋葱皮覆盖层/重生被误触发。
        self.spatial.note_index_dirty.set(true);
        self.grid_cache.clear();
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.editor_state.data.current_track
    }
}
