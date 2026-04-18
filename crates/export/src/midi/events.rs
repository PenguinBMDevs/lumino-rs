//! MIDI 轨道事件收集

use midly::TrackEvent;
use midly::TrackEventKind;

use super::types::MidiTrackData;

/// 收集轨道事件
pub fn collect_track_events(
    track_data: &MidiTrackData,
    events: &mut Vec<TrackEvent<'static>>,
    include_globals: bool,
) {
    // 音符事件
    for note in &track_data.notes {
        events.push(TrackEvent {
            delta: note.tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: midly::MidiMessage::NoteOn {
                    key: note.key.into(),
                    vel: note.velocity.into(),
                },
            },
        });

        let end_tick = note.tick.saturating_add(note.duration);
        events.push(TrackEvent {
            delta: end_tick.into(),
            kind: TrackEventKind::Midi {
                channel: note.channel.into(),
                message: midly::MidiMessage::NoteOff {
                    key: note.key.into(),
                    vel: 0.into(),
                },
            },
        });
    }

    // 速度事件 (全局事件)
    if include_globals {
        for tempo in &track_data.tempos {
            let tempo_value = midly::num::u24::try_from(tempo.tempo)
                .unwrap_or_else(|| midly::num::u24::new(500000));
            events.push(TrackEvent {
                delta: tempo.tick.into(),
                kind: TrackEventKind::Meta(midly::MetaMessage::Tempo(tempo_value)),
            });
        }
    }

    // 程序变更
    for pc in &track_data.program_changes {
        events.push(TrackEvent {
            delta: pc.tick.into(),
            kind: TrackEventKind::Midi {
                channel: pc.channel.into(),
                message: midly::MidiMessage::ProgramChange {
                    program: pc.program.into(),
                },
            },
        });
    }

    // 控制变更
    for cc in &track_data.control_changes {
        events.push(TrackEvent {
            delta: cc.tick.into(),
            kind: TrackEventKind::Midi {
                channel: cc.channel.into(),
                message: midly::MidiMessage::Controller {
                    controller: cc.controller.into(),
                    value: cc.value.into(),
                },
            },
        });
    }

    // 拍号事件 (全局事件)
    if include_globals {
        for ts in &track_data.time_signatures {
            events.push(TrackEvent {
                delta: ts.tick.into(),
                kind: TrackEventKind::Meta(midly::MetaMessage::TimeSignature(
                    ts.numerator,
                    ts.denominator,
                    ts.clocks_per_tick,
                    ts.notated_32nd_notes_per_beat,
                )),
            });
        }
    }

    // 调号事件 (全局事件)
    if include_globals {
        for ks in &track_data.key_signatures {
            events.push(TrackEvent {
                delta: ks.tick.into(),
                kind: TrackEventKind::Meta(midly::MetaMessage::KeySignature(ks.key, ks.is_major)),
            });
        }
    }
}
