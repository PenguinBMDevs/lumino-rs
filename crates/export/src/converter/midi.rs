//! MIDI 源文件导出功能
//!
//! 提供从 MIDI 文件或其他支持格式直接导出原始字节的能力。

use std::path::Path;

use crate::{ExportError, ExportResult};

/// 同步复制文件
pub fn copy_file_sync(source_path: &Path, save_path: &Path) -> ExportResult<u64> {
    std::fs::copy(source_path, save_path).map_err(ExportError::Io)
}

/// 从解析后的 MIDI 源文件导出
pub fn export_midi_from_parsed_midi_sync(source_path: &Path) -> ExportResult<Vec<u8>> {
    let extension = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    match extension.as_str() {
        "mid" | "midi" => std::fs::read(source_path).map_err(ExportError::Io),
        "lmpj" => {
            // 尝试读取 LMPJ 文件内是否包含原始 MIDI 数据（有些 LMPJ 可能未保存）
            let data = std::fs::read(source_path).map_err(ExportError::Io)?;
            let parsed: lumino_midi_loader::ParsedMidi = crate::format::decode_lmpj(&data)
                .map_err(|e| ExportError::InvalidData(format!("解析 LMPJ 失败: {e}")))?;

            // ParsedMidi 不再缓存原始 MIDI 字节（解析后即释放）。
            // 从原始路径读取，或从 MidiDocument 重新构建 MIDI。
            let original = parsed.info.path;
            if original.exists() {
                std::fs::read(&original).map_err(ExportError::Io)
            } else {
                Err(ExportError::InvalidData(
                    "当前 LMPJ 未包含原始 MIDI 数据，且原始文件不存在，无法导出标准 MIDI"
                        .to_string(),
                ))
            }
        }
        _ => Err(ExportError::InvalidData(format!(
            "不支持的 MIDI 源格式: {}",
            extension
        ))),
    }
}
