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
        self.editor.state.total_ticks = total_ticks as u32;
        self.editor.max_scroll_x = total_ticks * self.editor.state.zoom_x;
    }

    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor.state.ppq = ppq;
        self.editor.state.snap_precision = (ppq as f32) / 2.0;
        self.editor.state.default_note_length = (ppq as f32) / 2.0;
    }

    /// 加载音符到编辑器
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32, u8)]) {
        self.editor.notes.clear();
        for (tick, key, length, velocity) in notes {
            let editor_key = *key as u16;
            self.editor
                .notes
                .push(Note::new(*tick, editor_key, *length).with_velocity(*velocity));
        }
        self.invalidate_onion_skin_cache();
    }

    /// 设置当前音轨
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.sidebar.set_selected_track(track_idx);
        self.editor.switch_to_track(track_idx);
        self.invalidate_onion_skin_cache();
        self.update_playback_notes();
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32, u8)]) {
        self.editor.notes.clear();
        let mut track_notes = Vec::with_capacity(notes.len());

        for (tick, key, length, velocity) in notes {
            let editor_key = *key as u16;
            let note = Note::new(*tick, editor_key, *length).with_velocity(*velocity);
            self.editor.notes.push(note.clone());
            track_notes.push(note);
        }

        if !track_notes.is_empty() {
            self.editor.track_notes.insert(track_idx, track_notes);
        }

        self.editor.current_track = track_idx;
        self.invalidate_onion_skin_cache();
        self.update_playback_notes();
    }
}
