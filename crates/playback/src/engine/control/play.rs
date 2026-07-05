//! 播放/暂停/停止控制

use super::core::PlaybackEngine;

impl PlaybackEngine {
    /// 播放
    ///
    /// 事件由 set_current_track_notes/set_midi_events/seek 等操作触发重建，
    /// play() 自身不再重复重建，消除冗余操作。
    pub fn play(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.play();
        }
    }

    /// 暂停
    pub fn pause(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.pause();
        }
    }

    /// 停止
    pub fn stop(&mut self) {
        if let Some(mut playback) = self.lock_playback() {
            playback.stop();
        }
        // 重置所有音轨读取状态
        for state in &mut self.track_states {
            state.note_cursor = 0;
            state.pending_offs.clear();
        }
        self.control_event_cursor = 0;
        self.event_queue.clear();
        self.last_processed_tick = 0.0;
    }
}
