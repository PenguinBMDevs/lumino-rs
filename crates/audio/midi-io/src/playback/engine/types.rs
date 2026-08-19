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
    /// 事件时刻（tick）
    pub tick: f32,
    /// 事件类型
    pub event_type: EventType,
    /// 序列号，用于相同 tick 时保持顺序
    pub seq: u64,
}

/// 调度事件类型
#[derive(Debug, Clone)]
pub enum EventType {
    /// Note On 事件
    NoteOn {
        /// MIDI 通道
        channel: u8,
        /// 音高
        key: u8,
        /// 力度
        velocity: u8,
    },
    /// Note Off 事件
    NoteOff {
        /// MIDI 通道
        channel: u8,
        /// 音高
        key: u8,
    },
}

impl PartialEq for ScheduledEvent {
    fn eq(&self, other: &Self) -> bool {
        self.tick == other.tick && self.seq == other.seq
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
        // 先按 tick 排序，相同 tick 按 seq 排序
        other
            .tick
            .total_cmp(&self.tick)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

/// MIDI消息
#[derive(Debug, Clone)]
pub enum MidiMessage {
    /// Note On 消息
    NoteOn {
        /// MIDI 通道
        channel: u8,
        /// 音高
        key: u8,
        /// 力度
        velocity: u8,
    },
    /// Note Off 消息
    NoteOff {
        /// MIDI 通道
        channel: u8,
        /// 音高
        key: u8,
    },
    /// 控制器变化（CC）消息
    ControlChange {
        /// MIDI 通道
        channel: u8,
        /// 控制器编号
        controller: u8,
        /// 控制值
        value: u8,
    },
    /// 音色变换（Program Change）消息
    ProgramChange {
        /// MIDI 通道
        channel: u8,
        /// 音色编号
        program: u8,
    },
    /// 弯音消息
    PitchBend {
        /// MIDI 通道
        channel: u8,
        /// 弯音值（-1.0 到 1.0）
        value: f32,
    },
    /// 通道后触消息
    ChannelPressure {
        /// MIDI 通道
        channel: u8,
        /// 压力值
        pressure: u8,
    },
    /// 复音后触消息
    PolyPressure {
        /// MIDI 通道
        channel: u8,
        /// 音高
        key: u8,
        /// 压力值
        pressure: u8,
    },
}

/// MIDI轨道事件（用于播放调度）
#[derive(Debug, Clone)]
pub struct MidiTrackEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// MIDI消息
    pub message: MidiMessage,
}
