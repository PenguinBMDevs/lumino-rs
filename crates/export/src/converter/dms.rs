//! DMS 与 MIDI 格式间转换功能
//!
//! 提供 DMS → MIDI 和 MIDI → DMS 的同步转换函数。

use std::collections::HashMap;
use std::path::Path;

use crate::{ExportError, ExportResult};

/// 从 DMS 文件同步导出 MIDI
pub fn export_midi_from_dms_sync(source_path: &Path) -> ExportResult<Vec<u8>> {
    let bytes = std::fs::read(source_path).map_err(ExportError::Io)?;
    let root = lumino_dms::read_dms_file(&bytes)
        .map_err(|e| ExportError::InvalidData(format!("解析 DMS 文件失败: {e}")))?;
    let export_data = build_midi_export_from_dms(&root);
    crate::export_midi_to_bytes(&export_data)
        .map_err(|e| ExportError::MidiWrite(format!("导出失败: {e}")))
}

/// 从 MIDI 文件同步导出 DMS
pub fn export_dms_from_midi_sync(source_path: &Path) -> ExportResult<Vec<u8>> {
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension != "mid" && extension != "midi" {
        return Err(ExportError::InvalidData(
            "当前仅支持从标准 MIDI 文件导出 DMS，请先打开 .mid/.midi 文件".to_string(),
        ));
    }

    let export_data = build_dms_export_from_midi(source_path)?;
    crate::export_dms_to_bytes(&export_data)
        .map_err(|e| ExportError::DmsWrite(format!("导出失败: {e}")))
}

// ── DMS → MIDI 转换 ──

fn build_midi_export_from_dms(root: &lumino_dms::DmsCompositeNode) -> crate::midi::MidiExportData {
    use crate::midi::MidiExportOptions;
    use lumino_dms::DmsNodeType;

    let mut ppqn = lumino_midi_loader::constants::DEFAULT_PPQN;
    let mut tracks = Vec::new();

    for root_child in root.children.iter() {
        if root_child.type_id() == DmsNodeType::SONG_PPQN
            && let Some(value) = read_u64(root_child.as_ref())
        {
            ppqn = value.clamp(1, u16::MAX as u64) as u16;
        }

        if root_child.type_id() != DmsNodeType::TRACK {
            continue;
        }

        let track_data = parse_track_from_dms(root_child.as_ref());
        tracks.push(track_data);
    }

    crate::midi::MidiExportData {
        options: MidiExportOptions { format: 1, ppqn },
        tracks,
    }
}

/// 从 DMS 节点读取 u64 值
fn read_u64(node: &dyn lumino_dms::DmsNode) -> Option<u64> {
    use lumino_dms::DmsIntegerNode;
    let int_node = node.as_any().downcast_ref::<DmsIntegerNode>()?;
    let biguint = int_node.integer_data().to_biguint()?;
    let digits = biguint.to_u64_digits();
    match digits.len() {
        0 => Some(0),
        1 => Some(digits[0]),
        _ => None, // 超出 u64 范围的 BigInt 值
    }
}

/// 从 DMS 节点读取 f64 值
fn read_f64(node: &dyn lumino_dms::DmsNode) -> Option<f64> {
    use lumino_dms::DmsFloatNode;
    node.as_any()
        .downcast_ref::<DmsFloatNode>()
        .map(|n| n.number_data())
}

/// 从 DMS 节点读取字符串值
fn read_string(node: &dyn lumino_dms::DmsNode) -> Option<String> {
    use lumino_dms::DmsAnsiStringNode;
    node.as_any()
        .downcast_ref::<DmsAnsiStringNode>()
        .and_then(|n| n.string_data().ok())
}

/// 按类型查找子节点
fn child_by_type(
    node: &dyn lumino_dms::DmsNode,
    ty: lumino_dms::DmsNodeType,
) -> Option<&dyn lumino_dms::DmsNode> {
    node.children()
        .iter()
        .find(|child| child.type_id() == ty)
        .map(|child| child.as_ref())
}

/// 从子节点读取 u64 值，带默认值和范围限制
fn read_child_u64_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    child_by_type(parent, node_type)
        .and_then(read_u64)
        .unwrap_or(default)
        .clamp(min, max)
}

/// 从子节点读取 u32 值，带默认值和范围限制
fn read_child_u32_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: u64,
    min: u64,
    max: u64,
) -> u32 {
    read_child_u64_clamped(parent, node_type, default, min, max) as u32
}

/// 从子节点读取 f64 值，带默认值和范围限制
fn read_child_f64_clamped(
    parent: &dyn lumino_dms::DmsNode,
    node_type: lumino_dms::DmsNodeType,
    default: f64,
    min: f64,
    max: Option<f64>,
) -> f64 {
    child_by_type(parent, node_type)
        .and_then(read_f64)
        .unwrap_or(default)
        .clamp(min, max.unwrap_or(f64::MAX))
}

/// 从 DMS 节点解析单个音轨数据
fn parse_track_from_dms(track_node: &dyn lumino_dms::DmsNode) -> crate::midi::MidiTrackData {
    use crate::midi::MidiTrackData;
    use lumino_dms::DmsNodeType;

    let mut channel = 0u8;
    let mut name = None;
    let mut notes = Vec::new();
    let mut tempos = Vec::new();
    let mut control_changes = Vec::new();

    for track_child in track_node.children().iter() {
        match track_child.type_id() {
            DmsNodeType::TRACK_CHANNEL => {
                if let Some(ch) = read_u64(track_child.as_ref()) {
                    channel = ch.min(15) as u8;
                }
            }
            DmsNodeType::TRACK_NAME => {
                name = read_string(track_child.as_ref());
            }
            DmsNodeType::NOTE_EVENT => {
                if let Some(event) = parse_note_event(track_child.as_ref(), channel) {
                    notes.push(event);
                }
            }
            DmsNodeType::TEMPO_EVENT => {
                if let Some(event) = parse_tempo_event(track_child.as_ref()) {
                    tempos.push(event);
                }
            }
            DmsNodeType::CONTROL_EVENT => {
                if let Some(event) = parse_control_event(track_child.as_ref(), channel) {
                    control_changes.push(event);
                }
            }
            _ => {}
        }
    }

    MidiTrackData {
        notes,
        tempos,
        program_changes: Vec::new(),
        control_changes,
        time_signatures: Vec::new(),
        key_signatures: Vec::new(),
        name,
    }
}

/// 解析音符事件
fn parse_note_event(
    event_node: &dyn lumino_dms::DmsNode,
    channel: u8,
) -> Option<crate::midi::MidiNoteEvent> {
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let key = read_child_u32_clamped(event_node, DmsNodeType::NOTE_KEY_NUMBER, 60, 0, 127) as u8;
    let velocity =
        read_child_u32_clamped(event_node, DmsNodeType::NOTE_VELOCITY, 100, 0, 127) as u8;
    let duration =
        read_child_u32_clamped(event_node, DmsNodeType::NOTE_GATE, 1, 1, u32::MAX as u64);

    Some(crate::midi::MidiNoteEvent {
        tick,
        channel,
        key,
        velocity,
        duration,
    })
}

/// 解析速度事件
fn parse_tempo_event(event_node: &dyn lumino_dms::DmsNode) -> Option<crate::midi::MidiTempoEvent> {
    use crate::midi::{MidiTempoEvent, bpm_to_tempo};
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let bpm = read_child_f64_clamped(event_node, DmsNodeType::TEMPO_VALUE, 120.0, 1.0, None);

    Some(MidiTempoEvent {
        tick,
        tempo: bpm_to_tempo(bpm),
    })
}

/// 解析控制变更事件
fn parse_control_event(
    event_node: &dyn lumino_dms::DmsNode,
    channel: u8,
) -> Option<crate::midi::MidiControlChangeEvent> {
    use crate::midi::MidiControlChangeEvent;
    use lumino_dms::DmsNodeType;

    let tick = read_child_u32_clamped(event_node, DmsNodeType::ABS_TICK_POS, 0, 0, u32::MAX as u64);
    let controller = read_child_u32_clamped(event_node, DmsNodeType::CONTROL_TYPE, 0, 0, 127) as u8;
    let value = read_child_f64_clamped(
        event_node,
        DmsNodeType::CONTROL_VALUE,
        0.0,
        0.0,
        Some(127.0),
    )
    .round() as u8;

    Some(MidiControlChangeEvent {
        tick,
        channel,
        controller,
        value,
    })
}

// ── MIDI → DMS 转换 ──

fn build_dms_export_from_midi(source_path: &Path) -> ExportResult<crate::dms::DmsExportData> {
    use crate::dms::{DmsExportData, DmsExportOptions};
    use midly::{Smf, Timing};

    let bytes = std::fs::read(source_path).map_err(ExportError::Io)?;
    let smf = Smf::parse(&bytes)
        .map_err(|e| ExportError::InvalidData(format!("解析 MIDI 文件失败: {e}")))?;

    let ppqn = match smf.header.timing {
        Timing::Metrical(ticks) => Some(u16::from(ticks) as u32),
        _ => Some(lumino_midi_loader::constants::DEFAULT_PPQN as u32),
    };

    let mut tracks = Vec::new();
    for (index, track) in smf.tracks.iter().enumerate() {
        tracks.push(process_dms_track(index, track));
    }

    let song_name = source_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string());

    Ok(DmsExportData {
        options: DmsExportOptions {
            song_name,
            copyright: None,
            comment: None,
            ppqn,
        },
        tracks,
    })
}

fn process_dms_track(index: usize, track: &[midly::TrackEvent]) -> crate::dms::DmsTrack {
    use crate::dms::{DmsControlEvent, DmsNoteEvent, DmsTempoEvent, DmsTrack};
    use crate::midi::tempo_to_bpm;
    use midly::{MetaMessage, MidiMessage, TrackEventKind};

    let mut abs_tick = 0u32;
    let mut max_tick = 0u32;
    let mut name: Option<String> = None;
    let mut channel: Option<u8> = None;
    let mut notes = Vec::new();
    let mut tempos = Vec::new();
    let mut controls = Vec::new();
    let mut active_notes: HashMap<(u8, u8), (u32, u8)> = HashMap::new();

    for event in track {
        abs_tick = abs_tick.saturating_add(u32::from(event.delta));
        max_tick = max_tick.max(abs_tick);

        match &event.kind {
            TrackEventKind::Midi {
                channel: ch,
                message,
            } => {
                let ch_value = u8::from(*ch).min(15);
                channel.get_or_insert(ch_value);
                match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let key_value = *key;
                        let vel_value = u8::from(*vel);
                        if vel_value == 0 {
                            if let Some((start_tick, start_vel)) =
                                active_notes.remove(&(ch_value, key_value))
                            {
                                let gate = abs_tick.saturating_sub(start_tick).max(1);
                                notes.push(DmsNoteEvent {
                                    tick: start_tick as u64,
                                    key: key_value,
                                    velocity: start_vel,
                                    gate: gate as u64,
                                });
                            }
                        } else {
                            active_notes.insert((ch_value, key_value), (abs_tick, vel_value));
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let key_value = *key;
                        if let Some((start_tick, start_vel)) =
                            active_notes.remove(&(ch_value, key_value))
                        {
                            let gate = abs_tick.saturating_sub(start_tick).max(1);
                            notes.push(DmsNoteEvent {
                                tick: start_tick as u64,
                                key: key_value,
                                velocity: start_vel,
                                gate: gate as u64,
                            });
                        }
                    }
                    MidiMessage::Controller { controller, value } => {
                        controls.push(DmsControlEvent {
                            tick: abs_tick as u64,
                            control_type: u8::from(*controller),
                            value: u8::from(*value) as f64,
                            gate: 0.0,
                        });
                    }
                    _ => {}
                }
            }
            TrackEventKind::Meta(MetaMessage::Tempo(tempo)) => {
                tempos.push(DmsTempoEvent {
                    tick: abs_tick as u64,
                    tempo: tempo_to_bpm(u32::from(*tempo)),
                });
            }
            TrackEventKind::Meta(MetaMessage::TrackName(track_name)) => {
                name = Some(String::from_utf8_lossy(track_name).to_string());
            }
            _ => {}
        }
    }

    for ((_, key_value), (start_tick, start_vel)) in active_notes {
        let gate = max_tick.saturating_sub(start_tick).max(1);
        notes.push(DmsNoteEvent {
            tick: start_tick as u64,
            key: key_value,
            velocity: start_vel,
            gate: gate as u64,
        });
    }

    notes.sort_by_key(|n| n.tick);
    tempos.sort_by_key(|t| t.tick);
    controls.sort_by_key(|c| c.tick);

    DmsTrack {
        name: name.or_else(|| Some(format!("Track {}", index + 1))),
        port: 0,
        channel: channel.unwrap_or(0),
        is_drum: channel == Some(9),
        notes,
        tempos,
        controls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midly::num::u28;
    use midly::{MetaMessage, MidiMessage, TrackEvent, TrackEventKind};

    fn make_note_on(delta: u32, key: u8, vel: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: u28::from(delta),
            kind: TrackEventKind::Midi {
                channel: 0.into(),
                message: MidiMessage::NoteOn {
                    key: key.into(),
                    vel: vel.into(),
                },
            },
        }
    }

    fn make_note_off(delta: u32, key: u8) -> TrackEvent<'static> {
        TrackEvent {
            delta: u28::from(delta),
            kind: TrackEventKind::Midi {
                channel: 0.into(),
                message: MidiMessage::NoteOff {
                    key: key.into(),
                    vel: 0.into(),
                },
            },
        }
    }

    #[test]
    fn test_process_dms_track_empty() {
        let track = vec![];
        let result = process_dms_track(0, &track);
        assert_eq!(result.notes.len(), 0);
        assert_eq!(result.tempos.len(), 0);
        assert_eq!(result.name, Some("Track 1".to_string()));
    }

    #[test]
    fn test_process_dms_track_single_note() {
        let track = vec![make_note_on(0, 60, 100), make_note_off(480, 60)];
        let result = process_dms_track(0, &track);
        assert_eq!(result.notes.len(), 1);
        assert_eq!(result.notes[0].key, 60);
        assert_eq!(result.notes[0].velocity, 100);
        assert_eq!(result.notes[0].gate, 480);
        assert_eq!(result.notes[0].tick, 0);
    }

    #[test]
    fn test_process_dms_track_multiple_notes() {
        // Deltas are relative: event1→event2→event3 deltas accumulate
        let track = vec![
            make_note_on(0, 60, 100),
            make_note_on(0, 64, 80),
            make_note_off(240, 60), // abs_tick: 240, key60 gate=240
            make_note_off(0, 64),   // abs_tick: 240, key64 gate=240
        ];
        let result = process_dms_track(0, &track);
        assert_eq!(result.notes.len(), 2);
        assert_eq!(result.notes[0].key, 60);
        assert_eq!(result.notes[0].gate, 240);
        assert_eq!(result.notes[1].key, 64);
        assert_eq!(result.notes[1].gate, 240);
    }

    #[test]
    fn test_process_dms_track_noteon_with_vel_zero_is_noteoff() {
        // NoteOn with velocity 0 should be treated as NoteOff
        let track = vec![
            make_note_on(0, 60, 100),
            TrackEvent {
                delta: u28::from(480u32),
                kind: TrackEventKind::Midi {
                    channel: 0.into(),
                    message: MidiMessage::NoteOn {
                        key: 60.into(),
                        vel: 0.into(),
                    },
                },
            },
        ];
        let result = process_dms_track(0, &track);
        assert_eq!(result.notes.len(), 1);
        assert_eq!(result.notes[0].gate, 480);
    }

    #[test]
    fn test_process_dms_track_tempo() {
        use midly::num::u24;
        let track = vec![
            TrackEvent {
                delta: u28::from(0u32),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::from(500_000u32))),
            },
            make_note_on(0, 60, 100),
            make_note_off(480, 60),
        ];
        let result = process_dms_track(0, &track);
        // 500000 µs/beat → 120 BPM
        assert_eq!(result.tempos.len(), 1);
        assert!((result.tempos[0].tempo - 120.0).abs() < 0.01);
        assert_eq!(result.notes.len(), 1);
    }

    #[test]
    fn test_process_dms_track_track_name() {
        let track = vec![
            TrackEvent {
                delta: u28::from(0u32),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"Piano")),
            },
            make_note_on(0, 60, 100),
            make_note_off(480, 60),
        ];
        let result = process_dms_track(0, &track);
        assert_eq!(result.name, Some("Piano".to_string()));
    }

    #[test]
    fn test_process_dms_track_unclosed_notes() {
        // Note with no NoteOff — should be force-closed at max_tick
        let track = vec![make_note_on(0, 60, 100)];
        let result = process_dms_track(0, &track);
        assert_eq!(result.notes.len(), 1);
        assert_eq!(result.notes[0].key, 60);
        assert!(result.notes[0].gate >= 1); // force-closed with non-zero gate
    }

    #[test]
    fn test_process_dms_track_controller() {
        let track = vec![TrackEvent {
            delta: u28::from(0u32),
            kind: TrackEventKind::Midi {
                channel: 0.into(),
                message: MidiMessage::Controller {
                    controller: 7.into(), // volume
                    value: 100.into(),
                },
            },
        }];
        let result = process_dms_track(0, &track);
        assert_eq!(result.controls.len(), 1);
        assert_eq!(result.controls[0].control_type, 7);
        assert!((result.controls[0].value - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_process_dms_track_drum_channel() {
        // Channel 10 (index 9) should be marked as drum
        let track = vec![
            TrackEvent {
                delta: u28::from(0u32),
                kind: TrackEventKind::Midi {
                    channel: 9.into(),
                    message: MidiMessage::NoteOn {
                        key: 36.into(),
                        vel: 100.into(),
                    },
                },
            },
            TrackEvent {
                delta: u28::from(240u32),
                kind: TrackEventKind::Midi {
                    channel: 9.into(),
                    message: MidiMessage::NoteOff {
                        key: 36.into(),
                        vel: 0.into(),
                    },
                },
            },
        ];
        let result = process_dms_track(0, &track);
        assert!(result.is_drum, "channel 10 should be drum");
        assert_eq!(result.channel, 9);
    }
}
