use midly::{MetaMessage, MidiMessage, TrackEventKind};

use super::types::MidiEvent;

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
