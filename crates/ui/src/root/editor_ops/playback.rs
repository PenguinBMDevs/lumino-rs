//! 编辑器操作 - 播放管理

use crate::root::Root;

impl Root {
    /// 更新播放管理器中的音符数据和 MIDI 控制事件
    ///
    /// 收集所有音轨的音符和 CC/PC/PB 事件，同步到播放管理器。
    /// 在以下场景被调用：
    /// - 加载新 MIDI 文件 / 切换音轨
    /// - 编辑音符（绘制/移动/删除）
    pub fn update_playback_notes(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            let velocity_threshold = self.velocity_filter_threshold;

            let mut notes: Vec<crate::playback::NoteEvent> = Vec::new();

            // 当前音轨音符
            notes.extend(
                self.editor
                    .notes
                    .iter()
                    .filter(|note| note.velocity > velocity_threshold)
                    .map(|note| crate::playback::NoteEvent {
                        tick: note.tick,
                        channel: note.channel,
                        key: note.key as u8,
                        velocity: note.velocity,
                        length: note.length,
                    }),
            );

            // 其他音轨的音符
            for (track_idx, track_notes) in &self.editor.track_notes {
                if *track_idx == self.editor.current_track {
                    continue;
                }
                notes.extend(track_notes.iter().filter(|note| note.velocity > velocity_threshold).map(
                    |note| crate::playback::NoteEvent {
                        tick: note.tick,
                        channel: note.channel,
                        key: note.key as u8,
                        velocity: note.velocity,
                        length: note.length,
                    },
                ));
            }

            manager.set_notes(notes);

            // 同步所有音轨的 MIDI 控制事件
            let mut midi_events: Vec<crate::playback::MidiTrackEvent> = Vec::new();
            for events in self.track_midi_events.values() {
                midi_events.extend(events.clone());
            }
            if !midi_events.is_empty() {
                midi_events.sort_by(|a, b| a.tick.total_cmp(&b.tick));
                manager.set_midi_events(midi_events);
            }
        }
    }

    /// 重置播放管理器（加载新文件时调用）
    /// 下次 Play 会触发 init_playback_manager 重新收集数据
    pub fn reset_playback_manager(&mut self) {
        if self.playback_manager.is_some() {
            tracing::info!("Root: 重置播放管理器（新文件加载）");
            self.playback_manager = None;
        }
    }
}
