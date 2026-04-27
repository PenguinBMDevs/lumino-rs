use lumino_core::MidiEvent;
use midly::{Smf, TrackEventKind};
use std::collections::HashMap;

/// 音轨信息: (track_index, track_name, note_count)
pub type TrackInfo = (usize, Option<String>, u64);

/// 音轨音符映射: track_index -> notes (tick, key, length, velocity, channel)
pub type TrackNotesMap = HashMap<usize, Vec<(f32, u8, f32, u8, u8)>>;

/// 通用的 MIDI 音符解析函数
/// 将 MIDI 事件列表解析为音符列表 (tick, key, length, velocity, channel)
pub fn parse_midi_events_to_notes(events: &[MidiEvent]) -> Vec<(f32, u8, f32, u8, u8)> {
    // (channel, key) -> (start_tick, velocity, channel)
    let mut active_notes: HashMap<(u8, u8), (u32, u8, u8)> = HashMap::new();
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
                    // 如果音符已经在活动状态，先结束它
                    if let Some((start_tick, vel, ch)) = active_notes.remove(&(*channel, *key)) {
                        let length = tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, *key, length, vel, ch));
                    }
                    // 记录音符开始
                    active_notes.insert((*channel, *key), (*tick, *velocity, *channel));
                } else if let Some((start_tick, vel, ch)) = active_notes.remove(&(*channel, *key)) {
                    // velocity == 0 视为 NoteOff
                    let length = tick.saturating_sub(start_tick) as f32;
                    notes.push((start_tick as f32, *key, length, vel, ch));
                }
            }
            MidiEvent::NoteOff {
                track: _,
                tick,
                channel,
                key,
                velocity,
            } => {
                if let Some((start_tick, vel, ch)) = active_notes.remove(&(*channel, *key)) {
                    let length = tick.saturating_sub(start_tick) as f32;
                    // NoteOff 也有 velocity，但通常用 NoteOn 的 velocity
                    let _ = velocity;
                    notes.push((start_tick as f32, *key, length, vel, ch));
                }
            }
            _ => {}
        }
    }

    // 处理未关闭的音符（到音轨结束）
    let track_end_tick = events.iter().map(|e| e.tick()).max().unwrap_or(0);
    for ((_channel, key), (start_tick, vel, ch)) in active_notes {
        let length = track_end_tick.saturating_sub(start_tick) as f32;
        notes.push((start_tick as f32, key, length, vel, ch));
    }

    notes
}

/// 从 midly Smf 数据解析音符
pub fn parse_smf_to_notes(smf: &Smf) -> (Vec<TrackInfo>, TrackNotesMap) {
    let mut track_infos = Vec::new();
    let mut track_notes_map: TrackNotesMap = HashMap::new();

    for (track_idx, track) in smf.tracks.iter().enumerate() {
        let mut active_notes: std::collections::HashMap<(u8, u8), (u32, u8, u8)> =
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
                        let ch = channel.as_int();
                        let k = key.as_int();
                        let v = vel.as_int();
                        // 如果音符已经在活动状态，先结束它
                        if let Some((start_tick, start_vel, start_ch)) = active_notes.remove(&(ch, k)) {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, k, length, start_vel, start_ch));
                        }
                        active_notes.insert((ch, k), (abs_tick, v, ch));
                    } else {
                        // velocity == 0 视为 NoteOff
                        let ch = channel.as_int();
                        let k = key.as_int();
                        if let Some((start_tick, start_vel, start_ch)) = active_notes.remove(&(ch, k)) {
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

        track_infos.push((track_idx, track_name, notes.len() as u64));
    }

    (track_infos, track_notes_map)
}
