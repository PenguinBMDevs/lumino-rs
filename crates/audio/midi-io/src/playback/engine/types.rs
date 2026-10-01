//! 播放引擎类型定义

use std::cmp::Ordering;

/// REND-002：来源轨道端口 + MIDI 通道 → 合成层全局通道。
///
/// 单端口文档端口恒为 0 → 恒等；多端口 `port*16+channel`；超产品上限端口折叠
/// 到端口 15 块（与导出侧 `global_event_channel` 同口径）。
#[inline]
pub(crate) fn global_channel_for_track(port: u8, channel: u8) -> u16 {
    lumino_midi_model::multi_port::global_channel(
        lumino_midi_model::multi_port::effective_port(port),
        channel,
    )
}

/// 音符事件（用于播放调度）
#[derive(Debug, Clone)]
pub struct NoteEvent {
    /// 事件时刻（tick）
    pub tick: f32,
    /// 合成层全局通道（REND-002：`port*16+channel`）
    pub channel: u16,
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
        /// 合成层全局通道
        channel: u16,
        /// 音高
        key: u8,
        /// 力度
        velocity: u8,
    },
    /// Note Off 事件
    NoteOff {
        /// 合成层全局通道
        channel: u16,
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
///
/// 通道字段为**合成层全局通道**（REND-002：`port*16+channel`，u16）。
/// 单端口文档恒等（0..15），零行为变化。
#[derive(Debug, Clone)]
pub enum MidiMessage {
    /// Note On 消息
    NoteOn {
        /// 合成层全局通道
        channel: u16,
        /// 音高
        key: u8,
        /// 力度
        velocity: u8,
    },
    /// Note Off 消息
    NoteOff {
        /// 合成层全局通道
        channel: u16,
        /// 音高
        key: u8,
    },
    /// 控制器变化（CC）消息
    ControlChange {
        /// 合成层全局通道
        channel: u16,
        /// 控制器编号
        controller: u8,
        /// 控制值
        value: u8,
    },
    /// 音色变换（Program Change）消息
    ProgramChange {
        /// 合成层全局通道
        channel: u16,
        /// 音色编号
        program: u8,
    },
    /// 弯音消息
    PitchBend {
        /// 合成层全局通道
        channel: u16,
        /// 弯音值（-1.0 到 1.0）
        value: f32,
    },
    /// 通道后触消息
    ChannelPressure {
        /// 合成层全局通道
        channel: u16,
        /// 压力值
        pressure: u8,
    },
    /// 复音后触消息
    PolyPressure {
        /// 合成层全局通道
        channel: u16,
        /// 音高
        key: u8,
        /// 压力值
        pressure: u8,
    },
    /// REND-002 方案 B：打击乐模态切换（非 MIDI 线消息，仅软件合成器消费）。
    ///
    /// 由 Bank Select 约定在播放侧推导；`flush` 会转为
    /// `OutputConnection::set_percussion_mode`（外部设备默认忽略）。
    PercussionMode {
        /// 合成层全局通道
        channel: u16,
        /// `true` = 打击乐，`false` = 旋律
        on: bool,
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
