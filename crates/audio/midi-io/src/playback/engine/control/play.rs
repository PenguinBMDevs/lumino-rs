//! 播放/暂停/停止控制

use super::core::PlaybackEngine;
use crate::playback::PlaybackState;

impl PlaybackEngine {
    /// 播放
    ///
    /// 从停止态起播属于“全新一轮”：对齐时钟起点重建游标与当前轨队列，
    /// 否则上一轮消费掉的队列/游标停在尾部，下一轮从头播会无声。
    /// 事件由 set_current_track_notes/set_midi_events/seek 等操作触发重建，
    /// 暂停恢复（Paused→Playing）沿用剩余队列，此处不重建。
    pub fn play(&mut self) {
        let was_stopped = self.state() == PlaybackState::Stopped;
        // 先取起播 tick：停止态下 `current_tick()` 即上次 seek 位置（或 0），
        // 必须在 `playback.play()` 之前读取，避免墙钟启动后读到漂移值。
        let start_tick = self.current_tick();
        if let Some(mut playback) = self.lock_playback() {
            playback.play();
        }
        if was_stopped {
            self.reset_cursors_to(start_tick);
            self.rebuild_queue_from_current_track(Some(start_tick));
            self.pending_chase.clear();
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
        self.midi_event_cursor = 0;
        self.event_queue.clear();
        self.pending_chase.clear();
        self.last_processed_tick = 0.0;
        // 注意：此处不清掉 document，只清空“进度”，下次从停止态起播时
        // `play()` 会按起播 tick 重建当前轨队列（懒重建，避免 Stop 本身为大
        // 工程付出全量重建代价）。
    }
}
