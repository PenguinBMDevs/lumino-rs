//! MIDI 主导出逻辑

use std::path::Path;

use super::MidiExportData;
use super::tracks::build_midi_smf;
use crate::error::{ExportError, ExportResult};

/// 导出为 MIDI 文件
pub fn export_midi<P: AsRef<Path>>(path: P, data: &MidiExportData) -> ExportResult<()> {
    let buffer = export_midi_to_bytes(data)?;
    std::fs::write(path.as_ref(), buffer)?;
    Ok(())
}

/// 导出 MIDI 到字节数组
pub fn export_midi_to_bytes(data: &MidiExportData) -> ExportResult<Vec<u8>> {
    // 先收集轨道名称数据到 owned Vec 中，再建立引用切片
    // 利用 Rust drop 顺序（声明的逆序）：smf → name_bytes → name_buffers
    let name_buffers: Vec<Option<Vec<u8>>> = data
        .tracks
        .iter()
        .map(|t| t.name.as_ref().map(|n| n.clone().into_bytes()))
        .collect();

    let name_bytes: Vec<Option<&[u8]>> = name_buffers
        .iter()
        .map(|buf| buf.as_ref().map(|b| b.as_slice()))
        .collect();

    let smf = build_midi_smf(data, &name_bytes)?;

    let mut buffer = Vec::new();
    smf.write(&mut buffer)
        .map_err(|e| ExportError::MidiWrite(e.to_string()))?;

    Ok(buffer)
    // drop 顺序：buffer → smf → name_bytes → name_buffers（安全）
}
