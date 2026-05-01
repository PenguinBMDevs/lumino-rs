use midly::{Smf, TrackEventKind};
use std::collections::HashMap;

/// 音轨信息: (track_index, track_name, note_count)
pub type TrackInfo = (usize, Option<String>, u64);

/// 音轨音符映射: track_index -> notes (tick, key, length, velocity, channel)
pub type TrackNotesMap = HashMap<usize, Vec<(f32, u8, f32, u8, u8)>>;

/// 音轨MIDI控制事件
#[derive(Debug, Clone, Default)]
pub struct TrackMidiEvents {
    pub control_changes: Vec<(f32, u8, u8, u8)>, // tick, channel, controller, value
    pub program_changes: Vec<(f32, u8, u8)>,     // tick, channel, program
    pub pitch_bends: Vec<(f32, u8, f32)>,        // tick, channel, value (-1.0~1.0)
}

/// 从 midly Smf 数据解析音符和控制事件
pub fn parse_smf(
    smf: &Smf,
) -> (
    Vec<TrackInfo>,
    TrackNotesMap,
    HashMap<usize, TrackMidiEvents>,
) {
    let mut track_infos = Vec::new();
    let mut track_notes_map: TrackNotesMap = HashMap::new();
    let mut track_events_map: HashMap<usize, TrackMidiEvents> = HashMap::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut active_notes: std::collections::HashMap<(u8, u8), (u32, u8, u8)> =
            std::collections::HashMap::new();
        let mut notes = Vec::new();
        let mut track_name: Option<String> = None;
        let mut midi_events = TrackMidiEvents::default();
        let mut abs_tick: u32 = 0;

        for event in track {
            abs_tick += u32::from(event.delta);

            match event.kind {
                TrackEventKind::Meta(midly::MetaMessage::TrackName(name_bytes)) => {
                    track_name = String::from_utf8(name_bytes.to_vec()).ok();
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::NoteOn { key, vel },
                } => {
                    if vel > 0 {
                        let ch = channel.as_int();
                        let k = key.as_int();
                        let v = vel.as_int();
                        if let Some((start_tick, start_vel, start_ch)) =
                            active_notes.remove(&(ch, k))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, k, length, start_vel, start_ch));
                        }
                        active_notes.insert((ch, k), (abs_tick, v, ch));
                    } else {
                        let ch = channel.as_int();
                        let k = key.as_int();
                        if let Some((start_tick, start_vel, start_ch)) =
                            active_notes.remove(&(ch, k))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, k, length, start_vel, start_ch));
                        }
                    }
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::NoteOff { key, vel },
                } => {
                    let ch = channel.as_int();
                    let k = key.as_int();
                    if let Some((start_tick, start_vel, start_ch)) = active_notes.remove(&(ch, k)) {
                        let length = abs_tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, k, length, start_vel, start_ch));
                    }
                    let _ = vel;
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::Controller { controller, value },
                } => {
                    midi_events.control_changes.push((
                        abs_tick as f32,
                        channel.as_int(),
                        controller.as_int(),
                        value.as_int(),
                    ));
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::ProgramChange { program },
                } => {
                    midi_events.program_changes.push((
                        abs_tick as f32,
                        channel.as_int(),
                        program.as_int(),
                    ));
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::PitchBend { bend },
                } => {
                    let normalized = bend.as_int() as f32 / 8192.0;
                    midi_events
                        .pitch_bends
                        .push((abs_tick as f32, channel.as_int(), normalized));
                }
                _ => {}
            }
        }

        // 处理未关闭的音符
        let track_end_tick = abs_tick;
        for ((_channel, key), (start_tick, vel, ch)) in active_notes {
            let length = track_end_tick.saturating_sub(start_tick) as f32;
            notes.push((start_tick as f32, key, length, vel, ch));
        }

        if !notes.is_empty() {
            track_notes_map.insert(track_idx, notes.clone());
        }
        if !midi_events.control_changes.is_empty()
            || !midi_events.program_changes.is_empty()
            || !midi_events.pitch_bends.is_empty()
        {
            track_events_map.insert(track_idx, midi_events);
        }

        track_infos.push((track_idx, track_name, notes.len() as u64));
    }

    (track_infos, track_notes_map, track_events_map)
}
