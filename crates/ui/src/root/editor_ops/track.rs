//! 编辑器操作 - 音轨管理

use crate::editor::note::Note;
use crate::root::Root;

impl Root {
    /// 更新音轨列表（从 MIDI 导入）
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.set_total_ticks(total_ticks as u32);
    }

    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor.set_ppq(ppq);
        self.editor.set_snap_precision(ppq as f32);
        self.editor.set_default_note_length(ppq as f32);
    }

    /// 加载音符到编辑器
    /// notes: (tick, key, length, velocity, channel)
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32, u8, u8)]) {
        self.editor.editor_state.data.notes.clear();
        for &(tick, key, length, velocity, channel) in notes {
            self.editor
                .editor_state
                .data
                .notes
                .push_back(Note::from_raw(tick, key as u16, length, velocity, channel));
        }
        self.editor
            .track_note_indices
            .borrow_mut()
            .remove(&self.editor.editor_state.data.current_track);
        self.invalidate_onion_skin_cache();
        self.editor.mark_notes_changed();
    }

    /// 设置当前音轨
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.sidebar.set_selected_track(track_idx);
        self.editor.switch_to_track(track_idx);
        self.invalidate_onion_skin_cache();
        self.update_playback_notes();
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32, u8, u8)]) {
        self.editor.editor_state.data.notes.clear();
        let mut track_notes: im::Vector<Note> = im::Vector::new();

        for &(tick, key, length, velocity, channel) in notes {
            let note = Note::from_raw(tick, key as u16, length, velocity, channel);
            self.editor.editor_state.data.notes.push_back(note.clone());
            track_notes.push_back(note);
        }

        self.editor
            .editor_state
            .data
            .track_notes
            .insert(track_idx, track_notes);
        self.editor
            .track_note_indices
            .borrow_mut()
            .remove(&track_idx);

        self.editor.editor_state.data.current_track = track_idx;
        self.invalidate_onion_skin_cache();
        self.editor.mark_notes_changed();
        self.update_playback_notes();
    }

    /// 加载指定音轨的 MIDI 控制事件
    pub fn load_track_midi_events(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        if !events.is_empty() {
            self.track_midi_events.insert(track_idx, events);
            tracing::debug!(
                "Root: 音轨 {} 已加载 {} 个 MIDI 控制事件",
                track_idx,
                self.track_midi_events
                    .get(&track_idx)
                    .map_or(0, |v| v.len())
            );
        }
    }

    /// 预加载音轨 MIDI 控制事件到洋葱皮缓存
    pub fn load_track_midi_events_for_onion_skin(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        if !events.is_empty() {
            // 合并到已有事件（如果有）
            self.track_midi_events
                .entry(track_idx)
                .or_default()
                .extend(events);
        }
    }
}
