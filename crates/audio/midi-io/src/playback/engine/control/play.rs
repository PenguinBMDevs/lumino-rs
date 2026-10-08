//! 播放/暂停/停止控制

use super::core::PlaybackEngine;
use crate::playback::PlaybackState;

impl PlaybackEngine {
    /// 播放
    ///
    /// 从停止态起播属于“全新一轮”：对齐时钟起点重定位全部轨游标
    /// （PREF-006 A1 流式模型无预建队列，重定位即二分 + 悬挂扫描）。
    /// 暂停恢复（Paused→Playing）沿用现有游标，此处不重定位。
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
        self.pending_chase.clear();
        self.last_processed_tick = 0.0;
        // 注意：此处不清掉 document，只清空“进度”，下次从停止态起播时
        // `play()` 会按起播 tick 重定位全部轨游标（懒重定位，避免 Stop 本身
        // 为大工程付出全量代价）。
    }
}
