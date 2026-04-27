//! 播放引擎模块
//!
//! 负责音符调度和MIDI输出

use super::{Playback, PlaybackAccessor, PlaybackState};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::{Arc, Mutex};

/// 音符事件（用于播放调度）
#[derive(Debug, Clone)]
pub struct NoteEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// MIDI通道
    pub channel: u8,
    /// 音高
    pub key: u8,
    /// 力度
    pub velocity: u8,
    /// 音符长度（tick）
    pub length: f32,
}

/// 轨道MIDI事件（非音符，一次性事件）
#[derive(Debug, Clone)]
pub struct MidiTrackEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// MIDI消息
    pub message: MidiMessage,
}

/// 调度的播放事件（内部使用）
#[derive(Debug, Clone)]
struct ScheduledEvent {
    tick: f32,
    event_type: EventType,
}

#[derive(Debug, Clone)]
enum EventType {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: f32 },
    ChannelPressure { channel: u8, pressure: u8 },
    PolyPressure { channel: u8, key: u8, pressure: u8 },
}

impl PartialEq for ScheduledEvent {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick
    }
}

impl Eq for ScheduledEvent {}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other.tick.total_cmp(&self.tick)
    }
}

/// 播放引擎
pub struct PlaybackEngine {
    /// 播放器
    playback: Arc<Mutex<Playback>>,
    /// 待播放的事件队列（优先队列，按tick排序）
    event_queue: BinaryHeap<ScheduledEvent>,
    /// 所有音符
    notes: Vec<NoteEvent>,
    /// 非音符MIDI事件（CC/PC/PB等）
    midi_events: Vec<MidiTrackEvent>,
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
            event_queue: BinaryHeap::new(),
            notes: Vec::new(),
            midi_events: Vec::new(),
            looping: false,
            loop_range: None,
        }
    }

    /// 设置音符列表
    pub fn set_notes(&mut self, notes: Vec<NoteEvent>) {
        self.notes = notes;
        self.rebuild_queue();
    }

    /// 设置非音符MIDI事件列表
    pub fn set_midi_events(&mut self, events: Vec<MidiTrackEvent>) {
        self.midi_events = events;
        self.rebuild_queue();
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

    /// 重建事件队列
    fn rebuild_queue(&mut self) {
        self.event_queue.clear();

        // 调度音符事件
        for note in &self.notes {
            self.event_queue.push(ScheduledEvent {
                tick: note.tick,
                event_type: EventType::NoteOn {
                    channel: note.channel,
                    key: note.key,
                    velocity: note.velocity,
                },
            });

            self.event_queue.push(ScheduledEvent {
                tick: note.tick + note.length,
                event_type: EventType::NoteOff {
                    channel: note.channel,
                    key: note.key,
                },
            });
        }

        // 调度非音符MIDI事件
        for event in &self.midi_events {
            let event_type = match event.message {
                MidiMessage::NoteOn {
                    channel,
                    key,
                    velocity,
                } => EventType::NoteOn {
                    channel,
                    key,
                    velocity,
                },
                MidiMessage::NoteOff { channel, key } => EventType::NoteOff { channel, key },
                MidiMessage::ControlChange {
                    channel,
                    controller,
                    value,
                } => EventType::ControlChange {
                    channel,
                    controller,
                    value,
                },
                MidiMessage::ProgramChange { channel, program } => {
                    EventType::ProgramChange { channel, program }
                }
                MidiMessage::PitchBend { channel, value } => {
                    EventType::PitchBend { channel, value }
                }
                MidiMessage::ChannelPressure { channel, pressure } => {
                    EventType::ChannelPressure { channel, pressure }
                }
                MidiMessage::PolyPressure {
                    channel,
                    key,
                    pressure,
                } => EventType::PolyPressure {
                    channel,
                    key,
                    pressure,
                },
            };
            self.event_queue.push(ScheduledEvent {
                tick: event.tick,
                event_type,
            });
        }
    }

    /// 处理播放更新
    ///
    /// 返回：需要发送的MIDI消息列表
    pub fn update(&mut self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();

        let (current_tick, is_playing) = {
            let Some(playback) = self.lock_playback() else {
                return messages;
            };
            (
                playback.current_tick(),
                playback.state() == PlaybackState::Playing,
            )
        };

        if !is_playing {
            return messages;
        }

        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }

            let event = if let Some(e) = self.event_queue.pop() {
                e
            } else {
                tracing::error!("播放引擎: 事件队列状态不一致，peek 有值但 pop 失败");
                break;
            };

            match event.event_type {
                EventType::NoteOn {
                    channel,
                    key,
                    velocity,
                } => {
                    messages.push(MidiMessage::NoteOn {
                        channel,
                        key,
                        velocity,
                    });
                }
                EventType::NoteOff { channel, key } => {
                    messages.push(MidiMessage::NoteOff { channel, key });
                }
                EventType::ControlChange {
                    channel,
                    controller,
                    value,
                } => {
                    messages.push(MidiMessage::ControlChange {
                        channel,
                        controller,
                        value,
                    });
                }
                EventType::ProgramChange { channel, program } => {
                    messages.push(MidiMessage::ProgramChange { channel, program });
                }
                EventType::PitchBend { channel, value } => {
                    messages.push(MidiMessage::PitchBend { channel, value });
                }
                EventType::ChannelPressure { channel, pressure } => {
                    messages.push(MidiMessage::ChannelPressure { channel, pressure });
                }
                EventType::PolyPressure {
                    channel,
                    key,
                    pressure,
                } => {
                    messages.push(MidiMessage::PolyPressure {
                        channel,
                        key,
                        pressure,
                    });
                }
            }
        }

        if self.looping
            && let Some((loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            self.seek_playback(loop_start);
            self.rebuild_queue();
        }

        messages
    }

    /// 安全地跳转播放位置（内部辅助方法）
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
            self.rebuild_queue();
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
        self.rebuild_queue();
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.seek_playback(tick);
        self.rebuild_queue();
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

    fn lock_playback(&self) -> Option<std::sync::MutexGuard<Playback>> {
        self.playback.lock().ok()
    }
}

impl PlaybackAccessor for PlaybackEngine {
    fn playback(&self) -> &Arc<Mutex<Playback>> {
        &self.playback
    }
}

/// MIDI消息
#[derive(Debug, Clone)]
pub enum MidiMessage {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    PitchBend { channel: u8, value: f32 },
    ChannelPressure { channel: u8, pressure: u8 },
    PolyPressure { channel: u8, key: u8, pressure: u8 },
}

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

        assert_eq!(engine.event_queue.len(), 4);
    }

    #[test]
    fn test_midi_event_scheduling() {
        let playback = Arc::new(Mutex::new(Playback::new(480)));
        let mut engine = PlaybackEngine::new(playback);

        engine.set_midi_events(vec![
            MidiTrackEvent {
                tick: 0.0,
                message: MidiMessage::ProgramChange {
                    channel: 0,
                    program: 5,
                },
            },
            MidiTrackEvent {
                tick: 120.0,
                message: MidiMessage::ControlChange {
                    channel: 0,
                    controller: 64,
                    value: 127,
                },
            },
        ]);

        assert_eq!(engine.event_queue.len(), 2);
    }
}
