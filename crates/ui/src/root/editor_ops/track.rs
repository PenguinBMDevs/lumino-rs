//! 编辑器操作 - 音轨管理

use crate::editor::note::Note;
use crate::root::Root;

impl Root {
    /// 更新音轨列表（从 MIDI 导入）
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64, u8)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.set_total_ticks(total_ticks as u32);
        // 同步到走带视图（影响横向滚动最大长度）
        self.arrangement_view.viewport.total_ticks = total_ticks as u32;
    }

    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor.set_ppq(ppq);
        self.editor.set_snap_precision(ppq as f32);
        self.editor.set_default_note_length(ppq as f32);
        // PPQ 变更直接影响小节/拍线位置，必须立即失效网格和标尺缓存
        self.editor.grid_cache.clear();
        self.editor.ruler_cache.clear();
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
        self.editor.mark_notes_changed();
    }

    /// 设置当前音轨
    ///
    /// `open_panel` 控制是否在非 Arrangement 模式下强制打开侧边栏面板：
    /// - `true`：用户手动选轨时，确保面板打开以显示选中音轨
    /// - `false`：MIDI 加载等程序化操作，只刷新数据不强制弹出 UI
    pub fn set_current_track(&mut self, track_idx: usize, open_panel: bool) {
        self.sidebar
            .set_selected_track_with_panel(track_idx, open_panel);
        self.editor.switch_to_track(track_idx);
        self.update_playback_notes();

        // Conductor 轨道自动进入 Tempo 模式，普通轨道切回 Velocity
        let is_conductor = self
            .sidebar
            .tracks
            .first()
            .is_some_and(|t| t.id == track_idx && t.is_conductor);
        let panel = &mut self.editor.velocity_panel;
        if is_conductor {
            if !matches!(panel.edit_mode, crate::editor::velocity::EditMode::Tempo) {
                panel.edit_mode = crate::editor::velocity::EditMode::Tempo;
                tracing::debug!("Root: Conductor 轨道 → Tempo 编辑模式");
            }
        } else if matches!(panel.edit_mode, crate::editor::velocity::EditMode::Tempo) {
            panel.edit_mode = crate::editor::velocity::EditMode::Velocity;
            tracing::debug!("Root: 普通轨道 → Velocity 编辑模式");
        }
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
        self.editor.editor_state.data.mark_track_notes_changed();

        self.editor.editor_state.data.current_track = track_idx;
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
            self.playback.track_midi_events.insert(track_idx, events);
            tracing::debug!(
                "Root: 音轨 {} 已加载 {} 个 MIDI 控制事件",
                track_idx,
                self.playback
                    .track_midi_events
                    .get(&track_idx)
                    .map_or(0, |v| v.len())
            );
        }
    }

    /// 添加远程音轨（来自协作同步）
    pub fn add_remote_track(&mut self, track_idx: usize) {
        // 确保 sidebar tracks 足够容纳新音轨
        if track_idx >= self.sidebar.tracks.len() {
            self.sidebar.tracks.push(crate::sidebar::Track {
                id: track_idx,
                name: format!("Track {}", track_idx),
                channel: 0,
                display_label: format!("{:02}", track_idx + 1),
                is_conductor: false,
                can_delete: true,
                is_muted: false,
            });
            tracing::info!("协作: 已添加远程音轨 - track_index={}", track_idx);
        } else {
            tracing::warn!("协作: 远程音轨 track_index={} 已存在", track_idx);
        }
    }
}
