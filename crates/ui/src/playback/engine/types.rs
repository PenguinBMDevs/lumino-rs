//! 播放引擎类型定义

use std::cmp::Ordering;

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

/// 调度的音符事件（内部使用）
#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    pub tick: f32,
    pub event_type: EventType,
}

#[derive(Debug, Clone)]
pub enum EventType {
    NoteOn { channel: u8, key: u8, velocity: u8 },
    NoteOff { channel: u8, key: u8 },
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
        // 直接使用 total_cmp 避免 unwrap
        other.tick.total_cmp(&self.tick)
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

/// MIDI轨道事件（用于播放调度）
#[derive(Debug, Clone)]
pub struct MidiTrackEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// MIDI消息
    pub message: MidiMessage,
}
