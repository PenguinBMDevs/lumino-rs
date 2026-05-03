use midly::{MetaMessage, MidiMessage, TrackEventKind};

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MidiEvent {
    NoteOn {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        track: usize,
        tick: u32,
        channel: u8,
        key: u8,
        velocity: u8,
    },
    ControlChange {
        track: usize,
        tick: u32,
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        track: usize,
        tick: u32,
        channel: u8,
        program: u8,
    },
    PitchBend {
        track: usize,
        tick: u32,
        channel: u8,
        value: i16,
    },
    Tempo {
        track: usize,
        tick: u32,
        tempo: u32,
    },
    TimeSignature {
        track: usize,
        tick: u32,
        numerator: u8,
        denominator: u8,
    },
    KeySignature {
        track: usize,
        tick: u32,
        key: i8,
        is_major: bool,
    },
    TrackName {
        track: usize,
        tick: u32,
        name: String,
    },
    Other {
        track: usize,
        tick: u32,
        raw: Vec<u8>,
    },
}

impl MidiEvent {
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
                    key: key.as_int(),
                    velocity: vel.as_int(),
                }),
                MidiMessage::NoteOff { key, vel } => Some(MidiEvent::NoteOff {
                    track: track_index,
                    tick,
                    channel: ch,
                    key: key.as_int(),
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
