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

        // 2026-08 单一权威源：音符唯一权威是 document，切轨只需更新 current_track，
        // `current_track_notes()` 访问器会零拷贝读取新轨数据，无需缓存/回写。
        self.editor_state.data.current_track = track_idx;

        tracing::debug!(
            "Editor: loaded {} notes for track {}",
            self.editor_state.data.current_track_note_count(),
            track_idx
        );

        // 同步 NoteStore（降级 no-op，保留调用兼容）
        self.editor_state.data.sync_note_store();

        // 切换音轨时清除选中状态（通过 editor_state）
        self.selection_clear();
        self.editor_state.interaction.hover_state = None;
        self.editor_state.interaction.edit_state = super::EditState::Idle;
        // 丢弃未提交的批量拖动/复制：pending 的 selected 位图是旧轨的全局索引，
        // 换轨后继续保留会导致 ghost 渲染错位、提交时误伤新轨音符。
        self.pending_drag_state = None;
        self.pending_copy_drag_state = None;
        // 切轨只是切换当前显示的音轨（current_track_notes 换轨），
        // 并非用户编辑。需要重建空间索引并失效渲染缓存，
        // 但不能设置 notes_changed，否则会被 handle_action 误判为脏音轨，
        // 导致高精度洋葱皮覆盖层/重生被误触发。
        self.spatial.note_index_dirty.set(true);
        self.grid_cache.clear();

        // 主音轨增量对账：切轨后 GPU 布局将重建为新轨，
        // 旧轨残留的增量事件不可用 → 清队列 + 强制全量兜底
        self.editor_state.data.note_delta_events.clear();
        self.editor_state.data.note_delta_dirty = true;
    }

    /// 获取当前音轨索引
    pub fn current_track(&self) -> usize {
        self.editor_state.data.current_track
    }
}
