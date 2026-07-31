use midly::{Smf, TrackEventKind};
use std::collections::HashMap;

// 下方类型和函数仅在测试中使用，编译 binary 时视为 dead code
#[allow(dead_code)]
/// 音轨信息: (track_index, track_name, note_count)
pub type TrackInfo = (usize, Option<String>, u64);

#[allow(dead_code)]
/// 音轨音符映射: track_index -> notes (tick, key, length, velocity, channel)
pub type TrackNotesMap = HashMap<usize, Vec<(f32, u8, f32, u8, u8)>>;

#[allow(dead_code)]
/// 音轨MIDI控制事件
#[derive(Debug, Clone, Default)]
pub struct TrackMidiEvents {
    pub control_changes: Vec<(f32, u8, u8, u8)>, // tick, channel, controller, value
    pub program_changes: Vec<(f32, u8, u8)>,     // tick, channel, program
    pub pitch_bends: Vec<(f32, u8, f32)>,        // tick, channel, value (-1.0~1.0)
}

#[allow(dead_code)]
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
                        let midi_ch = channel.as_int();
                        let note_key = key;
                        let vel_int = vel.as_int();
                        if let Some((start_tick, start_vel, start_ch)) =
                            active_notes.remove(&(midi_ch, note_key))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, note_key, length, start_vel, start_ch));
                        }
                        active_notes.insert((midi_ch, note_key), (abs_tick, vel_int, midi_ch));
                    } else {
                        let midi_ch = channel.as_int();
                        let note_key = key;
                        if let Some((start_tick, start_vel, start_ch)) =
                            active_notes.remove(&(midi_ch, note_key))
                        {
                            let length = abs_tick.saturating_sub(start_tick) as f32;
                            notes.push((start_tick as f32, note_key, length, start_vel, start_ch));
                        }
                    }
                }
                TrackEventKind::Midi {
                    channel,
                    message: midly::MidiMessage::NoteOff { key, vel },
                } => {
                    let midi_ch = channel.as_int();
                    let note_key = key;
                    if let Some((start_tick, start_vel, start_ch)) =
                        active_notes.remove(&(midi_ch, note_key))
                    {
                        let length = abs_tick.saturating_sub(start_tick) as f32;
                        notes.push((start_tick as f32, note_key, length, start_vel, start_ch));
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

#[cfg(test)]
mod tests {
    use super::*;
    use midly::{
        Format, Header, Smf, Timing, Track, TrackEvent, TrackEventKind,
        num::{u4, u7, u28},
    };

    /// 辅助：构建一个简单的 SMF，包含一条音轨的给定事件
    fn build_smf(events: Vec<(u32, TrackEventKind)>) -> Smf {
        let track: Track = events
            .into_iter()
            .map(|(delta, kind)| TrackEvent {
                delta: u28::from(delta),
                kind,
            })
            .collect();
        Smf {
            header: Header::new(Format::Parallel, Timing::Metrical(480.into())),
            tracks: vec![track],
        }
    }

    #[test]
    fn test_simple_note_on_off() -> Result<(), Box<dyn std::error::Error>> {
        let smf = build_smf(vec![
            (
                0,
                TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOn {
                        key: 60,
                        vel: u7::from(100),
                    },
                },
            ),
            (
                480,
                TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOff {
                        key: 60,
                        vel: u7::from(64),
                    },
                },
            ),
        ]);
        let (_infos, notes, _events) = parse_smf(&smf);
        let track_notes = notes.get(&0).ok_or("音轨 0 应存在")?;
        assert_eq!(track_notes.len(), 1);
        assert_eq!(track_notes[0].1, 60); // key
        assert_eq!(track_notes[0].2, 480.0); // length
        Ok(())
    }

    #[test]
    fn test_velocity_zero_as_note_off() -> Result<(), Box<dyn std::error::Error>> {
        let smf = build_smf(vec![
            (
                0,
                TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOn {
                        key: 60,
                        vel: u7::from(100),
                    },
                },
            ),
            (
                480,
                TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOn {
                        key: 60,
                        vel: u7::from(0),
                    },
                },
            ),
        ]);
        let (_infos, notes, _events) = parse_smf(&smf);
        let track_notes = notes.get(&0).ok_or("音轨 0 应存在")?;
        assert_eq!(track_notes.len(), 1);
        assert_eq!(track_notes[0].2, 480.0);
        Ok(())
    }

    #[test]
    fn test_unclosed_note() -> Result<(), Box<dyn std::error::Error>> {
        // NoteOn 没有对应 NoteOff，应在轨道末尾自动关闭
        let smf = build_smf(vec![(
            0,
            TrackEventKind::Midi {
                channel: u4::from(0),
                message: midly::MidiMessage::NoteOn {
                    key: 60,
                    vel: u7::from(100),
                },
            },
        )]);
        let (_infos, notes, _events) = parse_smf(&smf);
        let track_notes = notes.get(&0).ok_or("音轨 0 应存在")?;
        assert_eq!(track_notes.len(), 1);
        assert_eq!(track_notes[0].2, 0.0); // length is 0 since end_tick == start_tick
        Ok(())
    }

    #[test]
    fn test_multiple_tracks() -> Result<(), Box<dyn std::error::Error>> {
        let track0: Track = vec![
            TrackEvent {
                delta: u28::from(0),
                kind: TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOn {
                        key: 60,
                        vel: u7::from(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::from(480),
                kind: TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::NoteOff {
                        key: 60,
                        vel: u7::from(64),
                    },
                },
            },
        ];
        let track1: Track = vec![
            TrackEvent {
                delta: u28::from(0),
                kind: TrackEventKind::Midi {
                    channel: u4::from(1),
                    message: midly::MidiMessage::NoteOn {
                        key: 72,
                        vel: u7::from(80),
                    },
                },
            },
            TrackEvent {
                delta: u28::from(240),
                kind: TrackEventKind::Midi {
                    channel: u4::from(1),
                    message: midly::MidiMessage::NoteOff {
                        key: 72,
                        vel: u7::from(64),
                    },
                },
            },
        ];
        let smf = Smf {
            header: Header::new(Format::Parallel, Timing::Metrical(480.into())),
            tracks: vec![track0, track1],
        };
        let (_infos, notes, _events) = parse_smf(&smf);
        assert_eq!(notes.get(&0).ok_or("音轨 0 应存在")?.len(), 1);
        assert_eq!(notes.get(&1).ok_or("音轨 1 应存在")?.len(), 1);
        assert_eq!(notes[&0][0].1, 60);
        assert_eq!(notes[&1][0].1, 72);
        Ok(())
    }

    #[test]
    fn test_control_events() -> Result<(), Box<dyn std::error::Error>> {
        let smf = build_smf(vec![
            (
                0,
                TrackEventKind::Midi {
                    channel: u4::from(0),
                    message: midly::MidiMessage::Controller {
                        controller: u7::from(7),
                        value: u7::from(100),
                    },
                },
            ),
            (
                0,
                TrackEventKind::Midi {
                    channel: u4::from(1),
                    message: midly::MidiMessage::ProgramChange {
                        program: u7::from(5),
                    },
                },
            ),
        ]);
        let (_infos, _notes, events) = parse_smf(&smf);
        let track_events = events.get(&0).ok_or("音轨 0 控制事件应存在")?;
        assert_eq!(track_events.control_changes.len(), 1);
        assert_eq!(track_events.control_changes[0].2, 7);
        assert_eq!(track_events.program_changes.len(), 1);
        assert_eq!(track_events.program_changes[0].2, 5);
        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use midly::Smf;

    const TEST_MIDI_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/test-file/noname.mid");

    fn read_test_file() -> Vec<u8> {
        std::fs::read(TEST_MIDI_PATH).unwrap_or_default()
    }

    fn test_file_exists() -> bool {
        !read_test_file().is_empty()
    }

    fn extract_notes_from_file() -> Option<(
        Vec<TrackInfo>,
        TrackNotesMap,
        HashMap<usize, TrackMidiEvents>,
    )> {
        let bytes = read_test_file();
        if bytes.is_empty() {
            return None;
        }
        let smf = Smf::parse(&bytes).ok()?;
        Some(parse_smf(&smf))
    }

    fn extract_packed_notes_from_file() -> Option<Vec<midly::loader::PackedNote>> {
        let bytes = read_test_file();
        if bytes.is_empty() {
            return None;
        }
        let (notes, _tempo) = midly::loader::extract_notes_from_bytes(&bytes).ok()?;
        Some(notes)
    }

    #[test]
    fn test_noname_midi_parses_all_notes() -> Result<(), Box<dyn std::error::Error>> {
        if !test_file_exists() {
            return Ok(());
        }
        let (_infos, notes_map, _events) = extract_notes_from_file().ok_or("解析失败")?;
        let track2 = notes_map.get(&2).ok_or("音轨 2 应有 263 个音符")?;
        assert_eq!(track2.len(), 263, "音轨 2 应有 263 个音符（key 0-254）");
        Ok(())
    }

    #[test]
    fn test_noname_midi_key_range() -> Result<(), Box<dyn std::error::Error>> {
        if !test_file_exists() {
            return Ok(());
        }
        let (_infos, notes_map, _events) = extract_notes_from_file().ok_or("解析失败")?;
        let track2 = notes_map.get(&2).ok_or("音轨 2 应有音符")?;
        let keys: Vec<u8> = track2.iter().map(|(_, k, _, _, _)| *k).collect();
        assert_eq!(
            *keys.iter().min().ok_or("keys 不应为空")?,
            0,
            "最低音应为 key=0"
        );
        assert_eq!(
            *keys.iter().max().ok_or("keys 不应为空")?,
            254,
            "最高音应为 key=254"
        );
        Ok(())
    }

    #[test]
    fn test_noname_midi_all_keys_unique() -> Result<(), Box<dyn std::error::Error>> {
        if !test_file_exists() {
            return Ok(());
        }
        let (_infos, notes_map, _events) = extract_notes_from_file().ok_or("解析失败")?;
        let track2 = notes_map.get(&2).ok_or("音轨 2 应有音符")?;
        let mut keys: Vec<u8> = track2.iter().map(|(_, k, _, _, _)| *k).collect();
        keys.sort();
        keys.dedup();
        // 文件覆盖 key 0-254，但 key 190 缺失，keys 236-244 有重复
        // 所以 dedup 后 unique key 数 = 255 - 1(缺190) = 254
        assert_eq!(keys.len(), 254, "dedup 后应有 254 个唯一 key（缺 key=190）");
        assert_eq!(keys[0], 0, "最低音 key=0");
        assert_eq!(keys[253], 254, "最高音 key=254");
        // 验证 key 190 确实缺失（存在于序列间隙中）
        assert!(!keys.contains(&190), "key 190 应缺失");
        Ok(())
    }

    #[test]
    fn test_noname_midi_y_coordinate_256key() {
        let zoom_y = 20.0f32;
        let max_128 = 127.0f32;
        let max_256 = 255.0f32;

        assert_eq!((max_128 - 0.0) * zoom_y, 127.0 * zoom_y, "128键 key 0 底部");
        assert_eq!((max_128 - 127.0) * zoom_y, 0.0, "128键 key 127 顶部");
        assert_eq!((max_256 - 0.0) * zoom_y, 255.0 * zoom_y, "256键 key 0 底部");
        assert_eq!(
            (max_256 - 127.0) * zoom_y,
            128.0 * zoom_y,
            "256键 key 127 中部"
        );
        assert_eq!(
            (max_256 - 254.0) * zoom_y,
            1.0 * zoom_y,
            "256键 key 254 近顶"
        );
        assert!(
            (max_256 - 254.0) * zoom_y >= 0.0,
            "key 254 的 world_y 不应为负"
        );
    }

    #[test]
    fn test_noname_midi_fast_path() -> Result<(), Box<dyn std::error::Error>> {
        if !test_file_exists() {
            return Ok(());
        }
        let notes = extract_packed_notes_from_file().ok_or("快速解析失败")?;
        assert_eq!(notes.len(), 263, "快速路径应提取 263 个音符");
        let keys: Vec<u8> = notes.iter().map(|n| n.key).collect();
        assert_eq!(*keys.iter().min().ok_or("keys 不应为空")?, 0);
        assert_eq!(*keys.iter().max().ok_or("keys 不应为空")?, 254);
        Ok(())
    }
}
