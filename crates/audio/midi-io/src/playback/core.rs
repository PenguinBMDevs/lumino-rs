//! 播放器核心模块

use parking_lot::{Mutex, MutexGuard};
use std::sync::Arc;
use std::time::Instant;

use crate::playback::state::PlaybackState;
use crate::playback::timeline::Timeline;

/// 提供 `Arc<Mutex<Playback>>` 访问的 trait
///
/// 用于消除 `lock_playback()` 方法的跨文件重复
/// 使用 parking_lot::Mutex 获得更好的性能（比 std::sync::Mutex 快 2-5 倍）
pub trait PlaybackAccessor {
    /// 返回播放器共享引用的 `Arc`
    fn playback(&self) -> &Arc<Mutex<Playback>>;

    /// 获取播放器的互斥锁保护守卫
    fn lock_playback(&self) -> Option<MutexGuard<'_, Playback>> {
        Some(self.playback().lock())
    }
}

/// 播放器
pub struct Playback {
    /// 播放状态
    state: PlaybackState,
    /// 时间线
    timeline: Timeline,
    /// 当前播放位置（tick）
    current_tick: f32,
    /// 播放开始时的真实时间
    play_start_time: Option<Instant>,
    /// 暂停时已播放的微秒数
    paused_microseconds: u64,
}

impl Playback {
    /// 创建新的播放器
    pub fn new(division: u16) -> Self {
        Self {
            state: PlaybackState::Stopped,
            timeline: Timeline::new(division),
            current_tick: 0.0,
            play_start_time: None,
            paused_microseconds: 0,
        }
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.state
    }

    /// 是否正在播放（用于播放线程决定高精度循环还是空闲阻塞）
    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }

    /// 获取当前播放位置（tick）
    pub fn current_tick(&self) -> f32 {
        if self.state == PlaybackState::Playing {
            if let Some(start_time) = self.play_start_time {
                let elapsed = start_time.elapsed().as_micros() as u64;
                let total_elapsed = self.paused_microseconds + elapsed;
                self.timeline.microseconds_to_tick(total_elapsed)
            } else {
                self.current_tick
            }
        } else {
            self.current_tick
        }
    }

    /// 设置时间线
    pub fn set_timeline(&mut self, timeline: Timeline) {
        self.timeline = timeline;
    }

    /// 设置速度变化
    pub fn set_tempo_changes(&mut self, changes: Vec<crate::playback::tempo::TempoChange>) {
        self.timeline.set_tempo_changes(changes);
    }

    /// 开始播放
    pub fn play(&mut self) {
        match self.state {
            PlaybackState::Stopped => {
                self.current_tick = 0.0;
                self.paused_microseconds = 0;
                self.play_start_time = Some(Instant::now());
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Paused => {
                // 从暂停恢复
                self.play_start_time = Some(Instant::now());
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Playing => {
                // 已经在播放，不做任何事
            }
        }
    }

    /// 暂停播放
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing
            && let Some(start_time) = self.play_start_time
        {
            let elapsed = start_time.elapsed().as_micros() as u64;
            self.paused_microseconds += elapsed;
            self.current_tick = self.timeline.microseconds_to_tick(self.paused_microseconds);
            self.play_start_time = None;
            self.state = PlaybackState::Paused;
        }
    }

    /// 停止播放
    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.current_tick = 0.0;
        self.play_start_time = None;
        self.paused_microseconds = 0;
    }

    /// 跳转到指定位置（tick）
    pub fn seek(&mut self, tick: f32) {
        self.current_tick = tick;
        if self.state == PlaybackState::Playing || self.state == PlaybackState::Paused {
            self.paused_microseconds = self.timeline.tick_to_microseconds(tick);
            if self.state == PlaybackState::Playing {
                self.play_start_time = Some(Instant::now());
            }
        }
    }

    /// 获取当前BPM
    pub fn current_bpm(&self) -> f64 {
        self.timeline.get_bpm_at(self.current_tick())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playback_state() {
        let mut playback = Playback::new(480);
        assert_eq!(playback.state(), PlaybackState::Stopped);

        playback.play();
        assert_eq!(playback.state(), PlaybackState::Playing);

        playback.pause();
        assert_eq!(playback.state(), PlaybackState::Paused);

        playback.stop();
        assert_eq!(playback.state(), PlaybackState::Stopped);
    }
}
