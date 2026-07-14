//! 位置/跳转/同步

use super::core::{PendingNoteOff, PlaybackEngine};
use lumino_midi_loader::MidiDocument;

impl PlaybackEngine {
    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback().map_or(0.0, |p| p.current_tick())
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.seek_playback(tick);
        // 重设游标到 seek_tick 位置
        self.reset_cursors_to(tick);
        // 重建当前轨事件队列
        self.rebuild_queue_from_current_track(Some(tick));
    }

    /// 将各音轨读取状态定位到指定 tick 位置
    pub(crate) fn reset_cursors_to(&mut self, tick: f32) {
        let Some(doc) = self.document.clone() else {
            return;
        };
        let seek_tick = tick as u32;
        for t in 0..self.track_states.len() {
            if t == self.current_track as usize {
                continue;
            }
            self.reset_track_state_to(t, seek_tick, &doc);
        }
        // 重置控制事件游标
        self.control_event_cursor = doc.control_events.partition_point(|e| e.tick < seek_tick);
        // 重置额外 MIDI 事件游标
        self.midi_event_cursor = self
            .midi_events
            .partition_point(|e| e.tick < seek_tick as f32);
        self.last_processed_tick = tick;
    }

    /// 将指定音轨的读取状态重置到 `seek_tick` 位置。
    ///
    /// `note_cursor` 指向第一颗 `start_tick >= seek_tick` 的音符；`pending_offs`
    /// 收集所有在 `seek_tick` 之前已经开始、但在 `seek_tick` 仍未结束的音符，
    /// 保证循环回绕或 seek 后这些音符的 NoteOff 能被正确发出。
    pub(crate) fn reset_track_state_to(
        &mut self,
        track: usize,
        seek_tick: u32,
        doc: &MidiDocument,
    ) {
        let notes = doc.track_notes(track);
        let state = &mut self.track_states[track];
        state.pending_offs.clear();
        state.note_cursor = notes.partition_point(|n| n.start_tick < seek_tick);
        for (i, note) in notes.iter().enumerate().take(state.note_cursor) {
            if note.end_tick >= seek_tick {
                state.pending_offs.push(PendingNoteOff {
                    end_tick: note.end_tick,
                    note_index: i,
                });
            }
        }
    }
}
