use serde::{Deserialize, Serialize};

/// MIDI事件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
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
        #[serde(rename = "isMajor")]
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
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        raw: Vec<u8>,
    },
}

impl From<&lumino_core::MidiEvent> for MidiEvent {
    fn from(event: &lumino_core::MidiEvent) -> Self {
        use lumino_core::MidiEvent as CoreEvent;

        match event {
            CoreEvent::NoteOn {
                track,
                tick,
                channel,
                key,
                velocity,
            } => Self::NoteOn {
                track: *track,
                tick: *tick,
                channel: *channel,
                key: *key,
                velocity: *velocity,
            },
            CoreEvent::NoteOff {
                track,
                tick,
                channel,
                key,
                velocity,
            } => Self::NoteOff {
                track: *track,
                tick: *tick,
                channel: *channel,
                key: *key,
                velocity: *velocity,
            },
            CoreEvent::ControlChange {
                track,
                tick,
                channel,
                controller,
                value,
            } => Self::ControlChange {
                track: *track,
                tick: *tick,
                channel: *channel,
                controller: *controller,
                value: *value,
            },
            CoreEvent::ProgramChange {
                track,
                tick,
                channel,
                program,
            } => Self::ProgramChange {
                track: *track,
                tick: *tick,
                channel: *channel,
                program: *program,
            },
            CoreEvent::Tempo { track, tick, tempo } => Self::Tempo {
                track: *track,
                tick: *tick,
                tempo: *tempo,
            },
            CoreEvent::TimeSignature {
                track,
                tick,
                numerator,
                denominator,
            } => Self::TimeSignature {
                track: *track,
                tick: *tick,
                numerator: *numerator,
                denominator: *denominator,
            },
            CoreEvent::KeySignature {
                track,
                tick,
                key,
                is_major,
            } => Self::KeySignature {
                track: *track,
                tick: *tick,
                key: *key,
                is_major: *is_major,
            },
            CoreEvent::TrackName { track, tick, name } => Self::TrackName {
                track: *track,
                tick: *tick,
                name: name.clone(),
            },
            CoreEvent::Other { track, tick, raw } => Self::Other {
                track: *track,
                tick: *tick,
                raw: raw.clone(),
            },
        }
    }
}
