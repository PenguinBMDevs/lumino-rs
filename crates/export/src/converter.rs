use std::collections::HashMap;
use std::path::Path;

pub fn copy_file_sync(source_path: &Path, save_path: &Path) -> Result<u64, String> {
    std::fs::copy(source_path, save_path).map_err(|e| format!("复制文件失败: {e}"))
}

pub fn export_midi_from_parsed_midi_sync(source_path: &Path) -> Result<Vec<u8>, String> {
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "mid" | "midi" => {
            std::fs::read(source_path).map_err(|e| format!("读取 MIDI 文件失败: {e}"))
        }
        "lmpj" => {
            // 尝试读取 LMPJ 文件内是否包含原始 MIDI 数据（有些 LMPJ 可能未保存）
            let data = std::fs::read(source_path).map_err(|e| format!("读取 LMPJ 失败: {e}"))?;
            let parsed: lumino_core::midi::ParsedMidi =
                crate::format::decode_lmpj(&data).map_err(|e| format!("解析 LMPJ 失败: {e}"))?;

            // 如果序列化数据中包含原始 midi bytes，则直接返回
            if let Some(midi_bytes) = parsed.midi_data {
                return Ok(midi_bytes);
            }

            // 否则尝试从保存的原始路径读取（如果存在）
            let original = parsed.info.path;
            if original.exists() {
                std::fs::read(&original).map_err(|e| format!("读取原始 MIDI 文件失败: {e}"))
            } else {
                Err(
                    "当前 LMPJ 未包含原始 MIDI 数据，且原始文件不存在，无法导出标准 MIDI"
                        .to_string(),
                )
            }
        }
        _ => Err(format!("不支持的 MIDI 源格式: {}", extension)),
    }
}

// LMPJ 保存逻辑已抽离到 `crate::lmpj` 模块。

pub fn export_midi_from_dms_sync(source_path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(source_path).map_err(|e| format!("读取 DMS 文件失败: {e}"))?;
    let root = lumino_dms::read_dms_file(&bytes).map_err(|e| format!("解析 DMS 文件失败: {e}"))?;
    let export_data = build_midi_export_from_dms(&root);
    crate::export_midi_to_bytes(&export_data).map_err(|e| format!("导出失败: {e}"))
}

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

fn build_midi_export_from_dms(root: &lumino_dms::DmsCompositeNode) -> crate::midi::MidiExportData {
    use crate::midi::{
        MidiControlChangeEvent, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTrackData,
        bpm_to_tempo,
    };
    use lumino_dms::{DmsAnsiStringNode, DmsFloatNode, DmsIntegerNode, DmsNode, DmsNodeType};

    fn read_u64(node: &dyn DmsNode) -> Option<u64> {
        node.as_any()
            .downcast_ref::<DmsIntegerNode>()
            .and_then(|n| n.integer_data().to_string().parse::<u64>().ok())
    }

    fn read_f64(node: &dyn DmsNode) -> Option<f64> {
        node.as_any()
            .downcast_ref::<DmsFloatNode>()
            .map(|n| n.number_data())
    }

    fn read_string(node: &dyn DmsNode) -> Option<String> {
        node.as_any()
            .downcast_ref::<DmsAnsiStringNode>()
            .and_then(|n| n.string_data().ok())
    }

    fn child_by_type(node: &dyn DmsNode, ty: DmsNodeType) -> Option<&dyn DmsNode> {
        node.children()
            .iter()
            .find(|child| child.type_id() == ty)
            .map(|child| child.as_ref())
    }

    let mut ppqn = 480u16;
    let mut tracks = Vec::new();

    for root_child in root.children() {
        if root_child.type_id() == DmsNodeType::SONG_PPQN
            && let Some(value) = read_u64(root_child.as_ref())
        {
            ppqn = value.clamp(1, u16::MAX as u64) as u16;
        }

        if root_child.type_id() != DmsNodeType::TRACK {
            continue;
        }

        let mut channel = 0u8;
        let mut name = None;
        let mut notes = Vec::new();
        let mut tempos = Vec::new();
        let mut control_changes = Vec::new();

        for track_child in root_child.children() {
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
                    let tick = child_by_type(track_child.as_ref(), DmsNodeType::ABS_TICK_POS)
                        .and_then(read_u64)
                        .unwrap_or(0)
                        .min(u32::MAX as u64) as u32;
                    let key = child_by_type(track_child.as_ref(), DmsNodeType::NOTE_KEY_NUMBER)
                        .and_then(read_u64)
                        .unwrap_or(60)
                        .min(127) as u8;
                    let velocity = child_by_type(track_child.as_ref(), DmsNodeType::NOTE_VELOCITY)
                        .and_then(read_u64)
                        .unwrap_or(100)
                        .min(127) as u8;
                    let duration = child_by_type(track_child.as_ref(), DmsNodeType::NOTE_GATE)
                        .and_then(read_u64)
                        .unwrap_or(1)
                        .max(1)
                        .min(u32::MAX as u64) as u32;

                    notes.push(MidiNoteEvent {
                        tick,
                        channel,
                        key,
                        velocity,
                        duration,
                    });
                }
                DmsNodeType::TEMPO_EVENT => {
                    let tick = child_by_type(track_child.as_ref(), DmsNodeType::ABS_TICK_POS)
                        .and_then(read_u64)
                        .unwrap_or(0)
                        .min(u32::MAX as u64) as u32;
                    let bpm = child_by_type(track_child.as_ref(), DmsNodeType::TEMPO_VALUE)
                        .and_then(read_f64)
                        .unwrap_or(120.0)
                        .max(1.0);

                    tempos.push(MidiTempoEvent {
                        tick,
                        tempo: bpm_to_tempo(bpm),
                    });
                }
                DmsNodeType::CONTROL_EVENT => {
                    let tick = child_by_type(track_child.as_ref(), DmsNodeType::ABS_TICK_POS)
                        .and_then(read_u64)
                        .unwrap_or(0)
                        .min(u32::MAX as u64) as u32;
                    let controller = child_by_type(track_child.as_ref(), DmsNodeType::CONTROL_TYPE)
                        .and_then(read_u64)
                        .unwrap_or(0)
                        .min(127) as u8;
                    let value = child_by_type(track_child.as_ref(), DmsNodeType::CONTROL_VALUE)
                        .and_then(read_f64)
                        .unwrap_or(0.0)
                        .round()
                        .clamp(0.0, 127.0) as u8;

                    control_changes.push(MidiControlChangeEvent {
                        tick,
                        channel,
                        controller,
                        value,
                    });
                }
                _ => {}
            }
        }

        tracks.push(MidiTrackData {
            notes,
            tempos,
            program_changes: Vec::new(),
            control_changes,
            time_signatures: Vec::new(),
            key_signatures: Vec::new(),
            name,
        });
    }

    crate::midi::MidiExportData {
        options: MidiExportOptions { format: 1, ppqn },
        tracks,
    }
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
        _ => Some(480),
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
