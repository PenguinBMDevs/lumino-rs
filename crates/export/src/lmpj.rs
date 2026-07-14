use std::collections::HashMap;
use std::path::Path;

use lumino_midi_loader::MidiDocument;

use crate::error::{ExportError, ExportResult};
use crate::midi::{
    MidiControlChangeEvent, MidiExportData, MidiExportOptions, MidiNoteEvent, MidiProgramChangeEvent,
    MidiTempoEvent, MidiTrackData,
};

/// 从 `MidiDocument.control_events` 按轨提取 PC/CC 事件。
///
/// 返回 `(program_changes, control_changes)` 按轨索引分组的 HashMap。
pub fn extract_pc_cc_events(
    doc: &MidiDocument,
) -> (
    HashMap<u16, Vec<MidiProgramChangeEvent>>,
    HashMap<u16, Vec<MidiControlChangeEvent>>,
) {
    let mut pc_by_track: HashMap<u16, Vec<MidiProgramChangeEvent>> = HashMap::new();
    let mut cc_by_track: HashMap<u16, Vec<MidiControlChangeEvent>> = HashMap::new();

    for ev in &doc.control_events {
        match ev.kind {
            0 => {
                // Control Change
                let (controller, value) = ev.as_control_change();
                cc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiControlChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        controller,
                        value,
                    });
            }
            1 => {
                // Program Change
                let program = ev.as_program_change();
                pc_by_track
                    .entry(ev.track)
                    .or_default()
                    .push(MidiProgramChangeEvent {
                        tick: ev.tick,
                        channel: ev.channel,
                        program,
                    });
            }
            _ => {} // Pitch Bend and others — not exported as PC/CC
        }
    }

    (pc_by_track, cc_by_track)
}

/// 从 MidiDocument 导出 MIDI 字节（含 tempo 变化、PC/CC 事件）。
///
/// LMPJ 是本机工程格式，保存时**必须**从内存中已解析的 document 重建，
/// 不应读取原始 .mid 文件。此函数确保所有用户编辑（tempo 等）和控制事件被保存。
fn midi_bytes_from_document(doc: &MidiDocument, division: u16) -> ExportResult<Vec<u8>> {
    let track_count = doc.track_count() as u16;

    // 从 document.tempo_changes 提取 tempo 事件
    let tempo_events: Vec<MidiTempoEvent> = doc
        .tempo_changes
        .iter()
        .map(|&(tick, bpm)| {
            let tempo_micros = if bpm > 0.0 {
                lumino_midi_loader::bpm_to_tempo(bpm as f64)
            } else {
                lumino_midi_loader::constants::DEFAULT_TEMPO_MICROS
            };
            MidiTempoEvent {
                tick,
                tempo: tempo_micros,
            }
        })
        .collect();

    // 提取 PC/CC 事件并按轨分组
    let (mut pc_by_track, mut cc_by_track) = extract_pc_cc_events(doc);

    let mut tracks: Vec<MidiTrackData> = (0..track_count)
        .map(|track_id| {
            let doc_notes = doc.get_track_notes(track_id);
            let midi_notes: Vec<MidiNoteEvent> = doc_notes
                .iter()
                .map(|&(tick, key, len, vel, ch)| MidiNoteEvent {
                    tick: (tick as u32).max(1),
                    channel: ch,
                    key,
                    velocity: vel,
                    duration: (len as u32).max(1),
                })
                .collect();
            MidiTrackData {
                notes: midi_notes,
                program_changes: pc_by_track.remove(&track_id).unwrap_or_default(),
                control_changes: cc_by_track.remove(&track_id).unwrap_or_default(),
                ..Default::default()
            }
        })
        .collect();

    // 第一个音轨附加 tempo 事件
    if let Some(first) = tracks.first_mut() {
        first.tempos = tempo_events;
    }

    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: division.max(1),
        },
        tracks,
    };

    crate::midi::export_midi_to_bytes(&export_data)
}

/// 同步保存 `ParsedMidi` 为 LMPJ。
///
/// 从内存中的 `MidiDocument` 重建 MIDI 字节并序列化，**不依赖原始 .mid 文件**。
pub fn save_parsed_midi_to_lmpj_sync(
    parsed: &lumino_midi_loader::ParsedMidi,
    path: &Path,
) -> ExportResult<()> {
    let midi_bytes = match parsed.document.as_ref() {
        Some(doc) => midi_bytes_from_document(doc, parsed.info.division)?,
        None => {
            return Err(ExportError::InvalidData(
                "ParsedMidi 没有加载 MidiDocument，无法保存 LMPJ".to_string(),
            ));
        }
    };

    let data_for_save = lumino_midi_loader::LmpjData {
        info: parsed.info.clone(),
        midi_data: Some(midi_bytes),
    };

    let compressed = crate::format::encode_lmpj(&data_for_save)?;

    std::fs::write(path, compressed)?;
    Ok(())
}

/// 异步保存 `ParsedMidi` 为 LMPJ（在 tokio 环境中使用）。
pub async fn save_parsed_midi_to_lmpj(
    parsed: &lumino_midi_loader::ParsedMidi,
    path: std::path::PathBuf,
) -> ExportResult<()> {
    let info = parsed.info.clone();
    let doc_ref = parsed.document.clone();

    let compressed = tokio::task::spawn_blocking(move || {
        let doc = doc_ref.ok_or_else(|| {
            ExportError::InvalidData("ParsedMidi 没有加载 MidiDocument，无法保存 LMPJ".to_string())
        })?;
        let midi_bytes = midi_bytes_from_document(&doc, info.division)?;
        let data_for_save = lumino_midi_loader::LmpjData {
            info,
            midi_data: Some(midi_bytes),
        };
        crate::format::encode_lmpj(&data_for_save)
    })
    .await
    .map_err(|e| crate::ExportError::Encoding(e.to_string()))??;

    tokio::fs::write(&path, compressed).await?;
    Ok(())
}

// 简短别名，便于调用方使用
/// 同步别名：`save_sync(parsed, path)`。
pub fn save_sync(parsed: &lumino_midi_loader::ParsedMidi, path: &Path) -> ExportResult<()> {
    save_parsed_midi_to_lmpj_sync(parsed, path)
}

/// 异步别名：`save(parsed, path)`。
pub async fn save(
    parsed: &lumino_midi_loader::ParsedMidi,
    path: std::path::PathBuf,
) -> ExportResult<()> {
    save_parsed_midi_to_lmpj(parsed, path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::{MidiDocument, NoteEvent, TrackManager};
    use midly::loader::PackedControlEvent;

    fn make_doc(control_events: Vec<PackedControlEvent>) -> MidiDocument {
        MidiDocument {
            notes: vec![
                vec![NoteEvent::new(0, 480, 60, 100, 0)],
                vec![NoteEvent::new(0, 480, 64, 100, 1)],
            ],
            tempo_changes: vec![(0, 120.0)],
            control_events,
            track_names: vec![Some("Track 0".into()), Some("Track 1".into())],
            total_ticks: 480,
            track_count: 2,
            tracks: TrackManager::new(2),
        }
    }

    #[test]
    fn test_extract_pc_cc_empty() {
        let doc = make_doc(vec![]);
        let (pc, cc) = extract_pc_cc_events(&doc);
        assert!(pc.is_empty());
        assert!(cc.is_empty());
    }

    #[test]
    fn test_extract_cc_events() {
        let doc = make_doc(vec![
            PackedControlEvent::control_change(0, 0, 0, 7, 100),   // track 0, CC7
            PackedControlEvent::control_change(480, 0, 0, 10, 64),  // track 0, CC10
            PackedControlEvent::control_change(0, 1, 1, 7, 80),     // track 1, CC7
        ]);
        let (pc, cc) = extract_pc_cc_events(&doc);

        assert!(pc.is_empty());
        assert_eq!(cc.len(), 2);
        assert_eq!(cc[&0].len(), 2);
        assert_eq!(cc[&1].len(), 1);

        assert_eq!(cc[&0][0].controller, 7);
        assert_eq!(cc[&0][0].value, 100);
        assert_eq!(cc[&0][0].tick, 0);
        assert_eq!(cc[&0][1].controller, 10);
        assert_eq!(cc[&0][1].value, 64);
        assert_eq!(cc[&0][1].tick, 480);
        assert_eq!(cc[&1][0].controller, 7);
        assert_eq!(cc[&1][0].value, 80);
    }

    #[test]
    fn test_extract_pc_events() {
        // PackedControlEvent::program_change(tick, track, channel, program)
        let doc = make_doc(vec![
            PackedControlEvent::program_change(0, 0, 0, 5),
            PackedControlEvent::program_change(0, 1, 1, 24),
        ]);
        let (pc, cc) = extract_pc_cc_events(&doc);

        assert!(cc.is_empty());
        assert_eq!(pc.len(), 2);
        assert_eq!(pc[&0].len(), 1);
        assert_eq!(pc[&0][0].program, 5);
        assert_eq!(pc[&1].len(), 1);
        assert_eq!(pc[&1][0].program, 24);
    }

    #[test]
    fn test_extract_mixed_pc_cc() {
        let doc = make_doc(vec![
            PackedControlEvent::program_change(0, 0, 0, 0),
            PackedControlEvent::control_change(0, 0, 0, 7, 100),
            PackedControlEvent::control_change(480, 0, 0, 10, 64),
            PackedControlEvent::program_change(0, 1, 1, 40),
        ]);
        let (pc, cc) = extract_pc_cc_events(&doc);

        assert_eq!(pc.len(), 2);
        assert_eq!(pc[&0].len(), 1);
        assert_eq!(pc[&1].len(), 1);
        assert_eq!(cc.len(), 1);
        assert_eq!(cc[&0].len(), 2);
    }

    #[test]
    fn test_extract_pitch_bend_ignored() {
        // Pitch bend (kind=2) should be ignored
        let doc = make_doc(vec![
            PackedControlEvent::pitch_bend(0, 0, 0, 0x2000),
        ]);
        let (pc, cc) = extract_pc_cc_events(&doc);
        assert!(pc.is_empty());
        assert!(cc.is_empty());
    }

    #[test]
    fn test_roundtrip_pc_cc_through_midi_export() {
        // 验证 extract_pc_cc_events 提取的事件能正确写入 MIDI 并重新解析
        let doc = make_doc(vec![
            PackedControlEvent::program_change(0, 0, 0, 19),
            PackedControlEvent::control_change(0, 0, 0, 7, 100),
            PackedControlEvent::control_change(480, 0, 0, 10, 64),
        ]);

        let (pc_by_track, cc_by_track) = extract_pc_cc_events(&doc);

        let track = MidiTrackData {
            notes: vec![MidiNoteEvent {
                tick: 0,
                channel: 0,
                key: 60,
                velocity: 100,
                duration: 480,
            }],
            tempos: vec![],
            program_changes: pc_by_track.into_values().flatten().collect(),
            control_changes: cc_by_track.into_values().flatten().collect(),
            time_signatures: vec![],
            key_signatures: vec![],
            name: None,
        };

        let export_data = MidiExportData {
            options: MidiExportOptions {
                format: 1,
                ppqn: 480,
            },
            tracks: vec![track],
        };

        let bytes = crate::midi::export_midi_to_bytes(&export_data).expect("export should succeed");
        let smf = midly::Smf::parse(&bytes).expect("should parse exported MIDI");
        assert_eq!(smf.tracks.len(), 1);

        let mut found_pc = false;
        let mut found_cc7 = false;
        let mut found_cc10 = false;
        for event in &smf.tracks[0] {
            match &event.kind {
                midly::TrackEventKind::Midi {
                    message: midly::MidiMessage::ProgramChange { program },
                    ..
                } => {
                    assert_eq!(u8::from(*program), 19);
                    found_pc = true;
                }
                midly::TrackEventKind::Midi {
                    message: midly::MidiMessage::Controller { controller, value },
                    ..
                } => match u8::from(*controller) {
                    7 => {
                        assert_eq!(u8::from(*value), 100);
                        found_cc7 = true;
                    }
                    10 => {
                        assert_eq!(u8::from(*value), 64);
                        found_cc10 = true;
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        assert!(found_pc, "roundtrip should preserve ProgramChange");
        assert!(found_cc7, "roundtrip should preserve CC7");
        assert!(found_cc10, "roundtrip should preserve CC10");
    }
}
