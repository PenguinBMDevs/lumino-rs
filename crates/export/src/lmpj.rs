use std::path::Path;
/// 同步保存 `ParsedMidi` 为 LMPJ。
pub fn save_parsed_midi_to_lmpj_sync(
    parsed: &lumino_core::midi::ParsedMidi,
    path: &Path,
) -> Result<(), String> {
    let data_for_save = lumino_core::midi::ParsedMidi {
        info: parsed.info.clone(),
        midi_data: None,
    };

    let compressed =
        crate::format::encode_lmpj(&data_for_save).map_err(|e| format!("压缩 LMPJ 失败: {e}"))?;

    std::fs::write(path, compressed).map_err(|e| format!("写入 LMPJ 失败: {e}"))
}

/// 异步保存 `ParsedMidi` 为 LMPJ（在 tokio 环境中使用）。
pub async fn save_parsed_midi_to_lmpj(
    parsed: &lumino_core::midi::ParsedMidi,
    path: std::path::PathBuf,
) -> Result<(), String> {
    let data_for_save = lumino_core::midi::ParsedMidi {
        info: parsed.info.clone(),
        midi_data: None,
    };

    let compressed = tokio::task::spawn_blocking(move || {
        crate::format::encode_lmpj(&data_for_save).map_err(|e| format!("压缩 LMPJ 失败: {e}"))
    })
    .await
    .map_err(|e| format!("压缩 LMPJ 失败: {e}"))??;

    tokio::fs::write(&path, compressed)
        .await
        .map_err(|e| format!("写入 LMPJ 失败: {e}"))
}

// 简短别名，便于调用方使用
/// 同步别名：`save_sync(parsed, path)`。
pub fn save_sync(parsed: &lumino_core::midi::ParsedMidi, path: &Path) -> Result<(), String> {
    save_parsed_midi_to_lmpj_sync(parsed, path)
}

/// 异步别名：`save(parsed, path)`。
pub async fn save(
    parsed: &lumino_core::midi::ParsedMidi,
    path: std::path::PathBuf,
) -> Result<(), String> {
    save_parsed_midi_to_lmpj(parsed, path).await
}
