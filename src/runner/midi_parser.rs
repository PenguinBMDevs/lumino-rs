use lumino_core::MidiEvent;
use midly::{Smf, TrackEventKind};
use std::collections::HashMap;

/// 音轨信息: (track_index, track_name, note_count)
pub type TrackInfo = (usize, Option<String>, u64);

/// 音轨音符映射: track_index -> notes
pub type TrackNotesMap = HashMap<usize, Vec<(f32, u8, f32)>>;

/// 通用的 MIDI 音符解析函数
/// 将 MIDI 事件列表解析为音符列表
pub fn parse_midi_events_to_notes(events: &[MidiEvent]) -> Vec<(f32, u8, f32)> {
    let mut active_notes: HashMap<(u8, u8), u32> = HashMap::new();
    let mut notes = Vec::new();

    for event in events {
        match event {
            MidiEvent::NoteOn {
                track: _,
                tick,
                channel,
                key,
                velocity,
            } => {
                if *velocity > 0 {
                    // 记录音符开始
                    active_notes.insert((*channel, *key), *tick);
                } else if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                    // velocity == 0 视为 NoteOff
                    let length = tick.saturating_sub(start_tick) as f32;
                    notes.push((start_tick as f32, *key, length));
                }
            }
            MidiEvent::NoteOff {
                track: _,
                tick,
                channel,
                key,
                ..
            } => {
                if let Some(start_tick) = active_notes.remove(&(*channel, *key)) {
                    let length = tick.saturating_sub(start_tick) as f32;
                    notes.push((start_tick as f32, *key, length));
                }
            }
            _ => {}
        }
    }

    // 处理未关闭的音符（到音轨结束）
    let track_end_tick = events.iter().map(|e| e.tick()).max().unwrap_or(0);
    for ((_channel, key), start_tick) in active_notes {
        let length = track_end_tick.saturating_sub(start_tick) as f32;
        notes.push((start_tick as f32, key, length));
    }

    notes
}

/// 从 midly Smf 数据解析音符
pub fn parse_smf_to_notes(smf: &Smf) -> (Vec<TrackInfo>, TrackNotesMap) {
    let mut track_infos = Vec::new();
    let mut track_notes_map: TrackNotesMap = HashMap::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut active_notes: std::collections::HashMap<(u8, u8), u32> =
            std::collections::HashMap::new();
        let mut notes = Vec::new();
        let mut track_name: Option<String> = None;
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
                        active_notes.insert((channel.as_int(), key.as_int()), abs_tick);
                    } else {
                        // velocity == 0 视为 NoteOff
                        if let Some(start_tick) =
                            active_notes.remove(&(channel.as_int(), key.as_int()))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, key.as_int(), length));
                        }
                    }
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::NoteOff { key, .. },
                } => {
                    if let Some(start_tick) = active_notes.remove(&(channel.as_int(), key.as_int()))
                    {
                        let length = abs_tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, key.as_int(), length));
                    }
                }
                _ => {}
            }
        }

        // 处理未关闭的音符
        let track_end_tick = abs_tick;
        for ((_channel, key), start_tick) in active_notes {
            let length = track_end_tick.saturating_sub(start_tick) as f32;
            notes.push((start_tick as f32, key, length));
        }

        if !notes.is_empty() {
            track_notes_map.insert(track_idx, notes.clone());
        }

        track_infos.push((track_idx, track_name, notes.len() as u64));
    }

    (track_infos, track_notes_map)
}
