use std::path::Path;

use crate::error::ExportResult;

/// 同步保存 `ParsedMidi` 为 LMPJ。
pub fn save_parsed_midi_to_lmpj_sync(
    parsed: &lumino_core::midi::ParsedMidi,
    path: &Path,
) -> ExportResult<()> {
    let data_for_save = lumino_core::LmpjData::from_parsed_midi(parsed);

    let compressed = crate::format::encode_lmpj(&data_for_save)?;

    std::fs::write(path, compressed)?;
    Ok(())
}

/// 异步保存 `ParsedMidi` 为 LMPJ（在 tokio 环境中使用）。
pub async fn save_parsed_midi_to_lmpj(
    parsed: &lumino_core::midi::ParsedMidi,
    path: std::path::PathBuf,
) -> ExportResult<()> {
    let data_for_save = lumino_core::LmpjData::from_parsed_midi(parsed);

    let compressed =
        tokio::task::spawn_blocking(move || crate::format::encode_lmpj(&data_for_save))
            .await
            .map_err(|e| crate::ExportError::Encoding(e.to_string()))??;

    tokio::fs::write(&path, compressed).await?;
    Ok(())
}

// 简短别名，便于调用方使用
/// 同步别名：`save_sync(parsed, path)`。
pub fn save_sync(parsed: &lumino_core::midi::ParsedMidi, path: &Path) -> ExportResult<()> {
    save_parsed_midi_to_lmpj_sync(parsed, path)
}

/// 异步别名：`save(parsed, path)`。
pub async fn save(
    parsed: &lumino_core::midi::ParsedMidi,
    path: std::path::PathBuf,
) -> ExportResult<()> {
    save_parsed_midi_to_lmpj(parsed, path).await
}
