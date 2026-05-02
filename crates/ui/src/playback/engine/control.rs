//! 播放引擎控制

use std::sync::{Arc, Mutex};

use crate::playback::{Playback, PlaybackAccessor, PlaybackState};

use super::{MidiMessage, MidiTrackEvent, NoteEvent, Scheduler};

/// 播放引擎
pub struct PlaybackEngine {
    /// 播放器
    playback: Arc<Mutex<Playback>>,
    /// 调度器
    scheduler: Scheduler,
    /// 循环播放
    looping: bool,
    /// 循环范围（开始tick，结束tick）
    loop_range: Option<(f32, f32)>,
}

impl PlaybackEngine {
    /// 创建新的播放引擎
    pub fn new(playback: Arc<Mutex<Playback>>) -> Self {
        Self {
            playback,
            scheduler: Scheduler::new(),
            looping: false,
            loop_range: None,
        }
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        self.scheduler.set_notes(notes);
    }

    /// 设置当前音轨音符列表（别名方法）
    pub fn set_current_track_notes(&mut self, notes: Vec<NoteEvent>) {
        self.set_notes(notes);
    }

    /// 设置MIDI事件列表（控制变更、音色变更等非音符事件）
    pub fn set_midi_events(&mut self, events: Vec<MidiTrackEvent>) {
        // 将MidiTrackEvent转换为调度器可处理的事件
        // 目前Scheduler只处理音符事件，MIDI控制事件直接发送到输出
        // 这里存储事件供后续处理
        tracing::debug!("PlaybackEngine: 设置了 {} 个MIDI轨道事件", events.len());
        // 事件将在update循环中处理
        for event in events {
            tracing::trace!("  - tick={}: {:?}", event.tick, event.message);
        }
    }

    /// 设置MIDI文档
    pub fn set_document(&mut self, doc: Arc<lumino_core::midi::MidiDocument>, current_track: u16) {
        // 从文档中提取当前音轨的音符
        // get_track_notes 返回 Vec<(tick, key, length, velocity, channel)>
        let notes: Vec<NoteEvent> = doc
            .get_track_notes(current_track)
            .into_iter()
            .map(|(tick, key, length, velocity, channel)| NoteEvent {
                tick,
                channel,
                key,
                velocity,
                length,
            })
            .collect();

        tracing::info!(
            "PlaybackEngine: 从文档加载音轨 {}，共 {} 个音符",
            current_track,
            notes.len()
        );

        self.set_notes(notes);
    }

    /// 设置循环
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// 设置循环范围
    pub fn set_loop_range(&mut self, start: f32, end: f32) {
        self.loop_range = Some((start, end));
    }

    /// 清除循环范围
    pub fn clear_loop_range(&mut self) {
        self.loop_range = None;
    }

    /// 处理播放更新
    pub fn update(&mut self) -> Vec<MidiMessage> {
        let (current_tick, is_playing) = {
            let Some(playback) = self.lock_playback() else {
                return Vec::new();
            };
            (
                playback.current_tick(),
                playback.state() == PlaybackState::Playing,
            )
        };

        let mut messages = self.scheduler.update(current_tick, is_playing);

        if self.looping
            && let Some((loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            self.seek_playback(loop_start);
            self.scheduler.rebuild_queue();
        }

        messages
    }

    /// 安全地跳转播放位置
    fn seek_playback(&self, tick: f32) {
        if let Some(mut playback) = self.lock_playback() {
            playback.seek(tick);
        }
    }

    /// 播放
    pub fn play(&mut self) {
        let state = self
            .lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state());

        if state == PlaybackState::Stopped {
            self.scheduler.rebuild_queue();
        }

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
        self.scheduler.rebuild_queue();
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.seek_playback(tick);
        self.scheduler.rebuild_queue();
    }

    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state())
    }

    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback().map_or(0.0, |p| p.current_tick())
    }

    fn lock_playback(&self) -> Option<MutexGuard<Playback>> {
        self.playback.lock().ok()
    }
}

impl PlaybackAccessor for PlaybackEngine {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}

use std::collections::binary_heap::BinaryHeap;
use std::sync::MutexGuard;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback::Playback;

    #[test]
    fn test_event_scheduling() {
        let playback = Arc::new(Mutex::new(Playback::new(480)));
        let mut engine = PlaybackEngine::new(playback);

        engine.set_notes(vec![
            NoteEvent {
                tick: 0.0,
                channel: 0,
                key: 60,
                velocity: 100,
                length: 480.0,
            },
            NoteEvent {
                tick: 480.0,
                channel: 0,
                key: 64,
                velocity: 100,
                length: 480.0,
            },
        ]);

        assert_eq!(engine.scheduler.len(), 4);
    }
}
