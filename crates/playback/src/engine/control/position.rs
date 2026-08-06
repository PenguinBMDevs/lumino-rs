//! 位置/跳转/同步

use super::core::{PendingNoteOff, PlaybackEngine};

impl PlaybackEngine {
    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback()
            .map_or(0.0, |playback_state| playback_state.current_tick())
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
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        let seek_tick = tick as u32;
        for track_idx in 0..self.track_states.len() {
            if track_idx == self.current_track as usize {
                continue;
            }
            let notes = doc.track_notes(track_idx);
            let state = &mut self.track_states[track_idx];
            state.pending_offs.clear();
            // ChunkedList::partition_point(tick) = 第一个 tick >= seek_tick 的索引
            // （等价于旧 `notes.partition_point(|n| n.start_tick < seek_tick)`）
            state.note_cursor = notes.partition_point(seek_tick);
            for (note_idx, note) in notes.iter().enumerate().take(state.note_cursor) {
                if note.end_tick >= seek_tick {
                    state.pending_offs.push(PendingNoteOff {
                        end_tick: note.end_tick,
                        note_index: note_idx,
                    });
                }
            }
        }
        // 重置控制事件游标（ChunkedList 分块二分）
        self.control_event_cursor = doc.control_events.partition_point(seek_tick);
        // 重置额外 MIDI 事件游标
        self.midi_event_cursor = self
            .midi_events
            .partition_point(|event| event.tick < seek_tick as f32);
        self.last_processed_tick = tick;
    }
}
