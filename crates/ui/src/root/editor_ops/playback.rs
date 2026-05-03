//! 编辑器操作 - 播放管理

use crate::playback::NoteEvent;
use crate::root::Root;
use std::sync::Arc;

impl Root {
    /// 更新播放管理器中的音符数据
    ///
    /// 当前轨从 `editor.notes` 实时读取发送到引擎；
    /// 其他音轨靠引擎直接从 document 流式读取，零额外内存。
    pub fn update_playback_notes(&mut self) {
        let Some(manager) = &mut self.playback_manager else {
            return;
        };

        // 更新 MIDI 文档引用（让引擎直接读 document 事件流）
        if let Some(doc) = &self.midi_document {
            manager.set_document(
                Arc::clone(doc),
                self.editor.editor_state.data.current_track as u16,
            );
        }

        // 当前音轨音符（编辑过的，从 editor.notes 实时送）
        let velocity_threshold = self.velocity_filter_threshold;
        let current_notes: Vec<NoteEvent> = self
            .editor
            .editor_state
            .data
            .notes
            .iter()
            .filter(|note| note.velocity > velocity_threshold)
            .map(|note| NoteEvent {
                tick: note.tick,
                channel: note.channel,
                key: note.key as u8,
                velocity: note.velocity,
                length: note.length,
            })
            .collect();
        manager.set_current_track_notes(current_notes);

        // 同步 MIDI 控制事件
        let mut midi_events: Vec<crate::playback::MidiTrackEvent> = Vec::new();
        for events in self.track_midi_events.values() {
            midi_events.extend(events.clone());
        }
        if !midi_events.is_empty() {
            midi_events.sort_by(|a, b| a.tick.total_cmp(&b.tick));
            manager.set_midi_events(midi_events);
        }
    }

    /// 重置播放管理器（加载新文件时调用）
    pub fn reset_playback_manager(&mut self) {
        if self.playback_manager.is_some() {
            tracing::info!("Root: 重置播放管理器（新文件加载）");
            self.playback_manager = None;
        }
    }
}
