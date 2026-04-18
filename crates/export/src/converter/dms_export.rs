use std::collections::HashMap;
use std::path::Path;

pub fn export_dms_from_midi_sync(source_path: &Path) -> Result<Vec<u8>, String> {
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if extension != "mid" && extension != "midi" {
        return Err("当前仅支持从标准 MIDI 文件导出 DMS，请先打开 .mid/.midi 文件".to_string());
    }

    let export_data = build_dms_export_from_midi(source_path)?;
    crate::export_dms_to_bytes(&export_data).map_err(|e| format!("导出失败: {e}"))
}

fn build_dms_export_from_midi(source_path: &Path) -> Result<crate::dms::DmsExportData, String> {
    use crate::dms::{
        DmsControlEvent, DmsExportData, DmsExportOptions, DmsNoteEvent, DmsTempoEvent, DmsTrack,
    };
    use crate::midi::tempo_to_bpm;
    use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};

    let bytes = std::fs::read(source_path).map_err(|e| format!("读取 MIDI 文件失败: {e}"))?;
    let smf = Smf::parse(&bytes).map_err(|e| format!("解析 MIDI 文件失败: {e}"))?;

    let ppqn = match smf.header.timing {
        Timing::Metrical(ticks) => Some(u16::from(ticks) as u32),
        _ => Some(1920),
    };

    let mut tracks = Vec::new();

    for (index, track) in smf.tracks.iter().enumerate() {
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
                            let key_value = u8::from(*key);
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
                            let key_value = u8::from(*key);
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

        tracks.push(DmsTrack {
            name: name.or_else(|| Some(format!("Track {}", index + 1))),
            port: 0,
            channel: channel.unwrap_or(0),
            is_drum: channel == Some(9),
            notes,
            tempos,
            controls,
        });
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
