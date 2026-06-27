use std::path::Path;

use lumino_midi_loader::MidiDocument;

use crate::error::{ExportError, ExportResult};
use crate::midi::{
    MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTempoEvent, MidiTrackData,
};

/// 从 MidiDocument 导出 MIDI 字节（含 tempo 变化）。
///
/// LMPJ 是本机工程格式，保存时**必须**从内存中已解析的 document 重建，
/// 不应读取原始 .mid 文件。此函数确保所有用户编辑（tempo 等）被保存。
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
