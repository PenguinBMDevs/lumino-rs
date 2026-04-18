use serde::{Deserialize, Serialize};

/// MIDI 事件类型（轻量级表示）
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            MidiEvent::Tempo { tick, .. } => *tick,
            MidiEvent::TimeSignature { tick, .. } => *tick,
            MidiEvent::KeySignature { tick, .. } => *tick,
            MidiEvent::TrackName { tick, .. } => *tick,
            MidiEvent::Other { tick, .. } => *tick,
        }
    }
}
