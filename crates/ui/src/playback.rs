//! 播放器模块
//!
//! 负责MIDI音符的播放，包括：
//! - 速度（Tempo）管理和BPM计算
//! - Tick到实际时间的转换
//! - 音符事件调度
//! - 播放状态管理

pub mod engine;
pub mod manager;

pub use engine::{MidiMessage, MidiTrackEvent, NoteEvent, PlaybackEngine};
pub use manager::PlaybackManager;

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// 提供 `Arc<Mutex<Playback>>` 访问的 trait
///
/// 用于消除 `lock_playback()` 方法的跨文件重复
pub trait PlaybackAccessor {
    fn playback(&self) -> &Arc<Mutex<Playback>>;

    fn lock_playback(&self) -> Option<MutexGuard<'_, Playback>> {
        match self.playback().lock() {
            Ok(guard) => Some(guard),
            Err(e) => {
                tracing::error!("Mutex 已被污染: {}", e);
                None
            }
        }
    }
}

/// 速度变化事件
#[derive(Debug, Clone)]
pub struct TempoChange {
    /// 发生时刻（tick）
    pub tick: f32,
    /// 速度值（微秒/四分音符）
    pub tempo: u32,
}

impl TempoChange {
    /// 从速度值创建（微秒/四分音符）
    pub fn from_tempo(tick: f32, tempo: u32) -> Self {
        Self { tick, tempo }
    }

    /// 从BPM创建
    pub fn from_bpm(tick: f32, bpm: f64) -> Self {
        let tempo = tempo_from_bpm(bpm);
        Self { tick, tempo }
    }

    /// 获取BPM值
    pub fn bpm(&self) -> f64 {
        bpm_from_tempo(self.tempo)
    }
}

/// 将BPM转换为tempo（微秒/四分音符）
pub fn tempo_from_bpm(bpm: f64) -> u32 {
    (60_000_000.0 / bpm).round() as u32
}

/// 将tempo（微秒/四分音符）转换为BPM
pub fn bpm_from_tempo(tempo: u32) -> f64 {
    60_000_000.0 / tempo as f64
}

/// 播放状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// 停止
    Stopped,
    /// 播放中
    Playing,
    /// 暂停
    Paused,
}

/// 时间线管理器
///
/// 负责根据速度变化将tick转换为实际时间
#[derive(Debug, Clone)]
pub struct Timeline {
    /// MIDI时间分辨率（ticks per quarter note）
    pub division: u16,
    /// 速度变化列表（按tick排序）
    tempo_changes: Vec<TempoChange>,
}

impl Timeline {
    /// 创建新的时间线
    pub fn new(division: u16) -> Self {
        Self {
            division,
            tempo_changes: vec![TempoChange::from_bpm(0.0, 120.0)], // 默认120 BPM
        }
    }

    /// 设置速度变化列表
    pub fn set_tempo_changes(&mut self, mut changes: Vec<TempoChange>) {
        if changes.is_empty() {
            changes.push(TempoChange::from_bpm(0.0, 120.0));
        }
        // 使用 total_cmp 进行安全的浮点数比较
        changes.sort_by(|a, b| a.tick.total_cmp(&b.tick));
        self.tempo_changes = changes;
    }

    /// 添加单个速度变化
    pub fn add_tempo_change(&mut self, change: TempoChange) {
        self.tempo_changes.push(change);
        // 使用 total_cmp 进行安全的浮点数比较
        self.tempo_changes.sort_by(|a, b| a.tick.total_cmp(&b.tick));
    }

    /// 获取当前BPM（在指定tick处）
    pub fn get_bpm_at(&self, tick: f32) -> f64 {
        let tempo = self.get_tempo_at(tick);
        bpm_from_tempo(tempo)
    }

    /// 获取当前tempo（在指定tick处）
    fn get_tempo_at(&self, tick: f32) -> u32 {
        // 默认120 BPM = 500000 微秒/拍
        const DEFAULT_TEMPO: u32 = 500_000;
        self.tempo_changes
            .iter()
            .rev()
            .find(|tc| tc.tick <= tick)
            .map(|tc| tc.tempo)
            .unwrap_or(DEFAULT_TEMPO)
    }

    /// 将tick转换为微秒
    pub fn tick_to_microseconds(&self, tick: f32) -> u64 {
        let mut current_tick = 0.0;
        let mut total_microseconds = 0u64;

        for (i, tempo_change) in self.tempo_changes.iter().enumerate() {
            let next_change_tick = self
                .tempo_changes
                .get(i + 1)
                .map(|tc| tc.tick)
                .unwrap_or(f32::MAX);

            if tick <= tempo_change.tick {
                // 目标在此速度段之前
                break;
            }

            let segment_end = tick.min(next_change_tick);
            let delta_ticks = segment_end - tempo_change.tick.max(current_tick);

            if delta_ticks > 0.0 {
                // 微秒 = (tick数 / division) * tempo
                let microseconds =
                    (delta_ticks as f64 / self.division as f64) * tempo_change.tempo as f64;
                total_microseconds += microseconds.round() as u64;
                current_tick = segment_end;
            }

            if tick <= segment_end {
                break;
            }
        }

        total_microseconds
    }

    /// 将微秒转换为tick（用于从时间反查位置）
    pub fn microseconds_to_tick(&self, target_microseconds: u64) -> f32 {
        let mut accumulated_microseconds = 0u64;
        let mut current_tick = 0.0;

        for (i, tempo_change) in self.tempo_changes.iter().enumerate() {
            let next_change_tick = self.tempo_changes.get(i + 1).map(|tc| tc.tick);

            if let Some(next_tick) = next_change_tick {
                // 计算这个速度段最多能消耗多少时间
                let segment_ticks = next_tick - tempo_change.tick;
                let segment_microseconds =
                    (segment_ticks as f64 / self.division as f64) * tempo_change.tempo as f64;
                let segment_microseconds_u64 = segment_microseconds.round() as u64;

                if accumulated_microseconds + segment_microseconds_u64 >= target_microseconds {
                    // 目标在此速度段内
                    let remaining = target_microseconds.saturating_sub(accumulated_microseconds);
                    let ticks_in_segment =
                        (remaining as f64 * self.division as f64) / tempo_change.tempo as f64;
                    return tempo_change.tick + ticks_in_segment as f32;
                }

                accumulated_microseconds += segment_microseconds_u64;
                current_tick = next_tick;
            } else {
                // 最后一个速度段，延伸到无限远
                let remaining = target_microseconds.saturating_sub(accumulated_microseconds);
                let ticks_in_segment =
                    (remaining as f64 * self.division as f64) / tempo_change.tempo as f64;
                return tempo_change.tick + ticks_in_segment as f32;
            }
        }

        current_tick
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
    pub fn set_tempo_changes(&mut self, changes: Vec<TempoChange>) {
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
    fn test_tempo_conversion() {
        let bpm = 120.0;
        let tempo = tempo_from_bpm(bpm);
        assert_eq!(tempo, 500_000);
        assert_eq!(bpm_from_tempo(tempo), bpm);
    }

    #[test]
    fn test_timeline_single_tempo() {
        let timeline = Timeline::new(480);
        // 默认120 BPM = 500000 微秒/拍
        // 480 ticks = 1拍 = 500000微秒
        let microseconds = timeline.tick_to_microseconds(480.0);
        assert_eq!(microseconds, 500_000);

        let tick = timeline.microseconds_to_tick(500_000);
        assert!((tick - 480.0).abs() < 0.1);
    }

    #[test]
    fn test_timeline_tempo_changes() {
        let mut timeline = Timeline::new(480);
        timeline.set_tempo_changes(vec![
            TempoChange::from_bpm(0.0, 120.0),   // 0-480: 120 BPM
            TempoChange::from_bpm(480.0, 240.0), // 480+: 240 BPM
        ]);

        // 前480 ticks: 120 BPM = 500000微秒
        assert_eq!(timeline.tick_to_microseconds(480.0), 500_000);

        // 480-960: 240 BPM = 250000微秒
        // 总共: 500000 + 250000 = 750000
        assert_eq!(timeline.tick_to_microseconds(960.0), 750_000);
    }

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
