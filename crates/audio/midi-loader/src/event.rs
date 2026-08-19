//! MIDI 事件类型与解析工具。
//!
//! `MidiEvent` 用于流式/顺序消费 MIDI 数据时的轻量级表示，
//! `parse_track_event_kind` 将 `midly` 的轨道事件转换为 `MidiEvent`。

use midly::{MetaMessage, MidiMessage, TrackEventKind};

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MidiEvent {
    /// 音符开启（Note On）。
    NoteOn {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// MIDI 通道（0-15）。
        channel: u8,
        /// 音高（MIDI 音高数字）。
        key: u8,
        /// 力度（0-127）。
        velocity: u8,
    },
    /// 音符关闭（Note Off）。
    NoteOff {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// MIDI 通道（0-15）。
        channel: u8,
        /// 音高（MIDI 音高数字）。
        key: u8,
        /// 力度（0-127）。
        velocity: u8,
    },
    /// 控制变更（Control Change）。
    ControlChange {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// MIDI 通道（0-15）。
        channel: u8,
        /// 控制器编号（0-127）。
        controller: u8,
        /// 控制值（0-127）。
        value: u8,
    },
    /// 音色变更（Program Change）。
    ProgramChange {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// MIDI 通道（0-15）。
        channel: u8,
        /// 音色编号（0-127）。
        program: u8,
    },
    /// 弯音（Pitch Bend）。
    PitchBend {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// MIDI 通道（0-15）。
        channel: u8,
        /// 弯音值（-8192..=8191）。
        value: i16,
    },
    /// 速度变更（Tempo）。
    Tempo {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// 速度（微秒每拍）。
        tempo: u32,
    },
    /// 拍号变更（Time Signature）。
    TimeSignature {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// 拍号分子。
        numerator: u8,
        /// 拍号分母（以 2 的幂表示）。
        denominator: u8,
    },
    /// 调号变更（Key Signature）。
    KeySignature {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// 调号偏移（-7..=7，升号为正、降号为负）。
        key: i8,
        /// 是否为大调（`false` 为小调）。
        #[serde(rename = "isMajor")]
        is_major: bool,
    },
    /// 音轨名称（Track Name）。
    TrackName {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// 音轨名称。
        name: String,
    },
    /// 其他未归类的原始事件（SysEx / Escape 等）。
    Other {
        /// 事件所属音轨索引。
        track: usize,
        /// 事件所在的 tick。
        tick: u32,
        /// 原始字节数据。
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        raw: Vec<u8>,
    },
}

impl MidiEvent {
    /// 返回事件的 tick（辅助对事件流按时间排序）。
    pub fn tick(&self) -> u32 {
        match self {
            MidiEvent::NoteOn { tick, .. } => *tick,
            MidiEvent::NoteOff { tick, .. } => *tick,
            MidiEvent::ControlChange { tick, .. } => *tick,
            MidiEvent::ProgramChange { tick, .. } => *tick,
            MidiEvent::PitchBend { tick, .. } => *tick,
            MidiEvent::Tempo { tick, .. } => *tick,
            MidiEvent::TimeSignature { tick, .. } => *tick,
            MidiEvent::KeySignature { tick, .. } => *tick,
            MidiEvent::TrackName { tick, .. } => *tick,
            MidiEvent::Other { tick, .. } => *tick,
        }
    }
}

/// 将 midly 的 TrackEventKind 解析为 MidiEvent
///
/// 这是一个纯函数，不依赖任何结构体状态，可以在任何地方使用
pub fn parse_track_event_kind(
    track_index: usize,
    tick: u32,
    kind: &TrackEventKind,
) -> Option<MidiEvent> {
    match kind {
        TrackEventKind::Midi { channel, message } => {
            let ch = channel.as_int();
            match message {
                MidiMessage::NoteOn { key, vel } => Some(MidiEvent::NoteOn {
                    track: track_index,
                    tick,
                    channel: ch,
                    key: *key,
                    velocity: vel.as_int(),
                }),
                MidiMessage::NoteOff { key, vel } => Some(MidiEvent::NoteOff {
                    track: track_index,
                    tick,
                    channel: ch,
                    key: *key,
                    velocity: vel.as_int(),
                }),
                MidiMessage::Controller { controller, value } => Some(MidiEvent::ControlChange {
                    track: track_index,
                    tick,
                    channel: ch,
                    controller: controller.as_int(),
                    value: value.as_int(),
                }),
                MidiMessage::ProgramChange { program } => Some(MidiEvent::ProgramChange {
                    track: track_index,
                    tick,
                    channel: ch,
                    program: program.as_int(),
                }),
                MidiMessage::PitchBend { bend } => Some(MidiEvent::PitchBend {
                    track: track_index,
                    tick,
                    channel: ch,
                    value: bend.as_int(),
                }),
                _ => None,
            }
        }
        TrackEventKind::Meta(meta) => match meta {
            MetaMessage::Tempo(tempo) => Some(MidiEvent::Tempo {
                track: track_index,
                tick,
                tempo: tempo.as_int(),
            }),
            MetaMessage::TimeSignature(num, den, _, _) => Some(MidiEvent::TimeSignature {
                track: track_index,
                tick,
                numerator: *num,
                denominator: *den,
            }),
            MetaMessage::KeySignature(key, is_major) => Some(MidiEvent::KeySignature {
                track: track_index,
                tick,
                key: *key,
                is_major: *is_major,
            }),
            MetaMessage::TrackName(name) => Some(MidiEvent::TrackName {
                track: track_index,
                tick,
                name: String::from_utf8_lossy(name).to_string(),
            }),
            _ => None,
        },
        TrackEventKind::SysEx(data) => Some(MidiEvent::Other {
            track: track_index,
            tick,
            raw: data.to_vec(),
        }),
        TrackEventKind::Escape(data) => Some(MidiEvent::Other {
            track: track_index,
            tick,
            raw: data.to_vec(),
        }),
    }
}
