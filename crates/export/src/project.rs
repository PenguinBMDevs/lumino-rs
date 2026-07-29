//! 工程文件格式兼容层
//!
//! 核心类型已迁移到 `lumino-core`，本模块仅保留：
//! - 旧版 LMPJ 文件兼容加载
//! - 供 Runner 使用的便捷扩展（`LuminoProject -> ParsedMidi`）

use std::path::{Path, PathBuf};

pub use lumino_core::project::*;

// 重新导出核心保存函数，保持 `lumino_export::project::save_to_archive` 等路径可用
pub use lumino_core::project::save::{save_to_archive, save_to_folder};

/// 将 `LuminoProject` 转换为 `ParsedMidi`，供 Runner 复用现有事件流。
pub fn project_to_parsed_midi(
    project: &LuminoProject,
    original_path: impl Into<PathBuf>,
) -> crate::ExportResult<lumino_midi_loader::ParsedMidi> {
    let document = project
        .to_midi_document()
        .map_err(crate::ExportError::from)?;
    let total_notes: u64 = document.notes.iter().map(|v| v.len() as u64).sum();

    let info = lumino_midi_loader::MidiInfo {
        path: original_path.into(),
        track_count: document.track_count,
        total_notes,
        duration_ticks: document.total_ticks,
        division: project.metadata.audio.division,
        parse_progress: Some(100.0),
    };

    Ok(lumino_midi_loader::ParsedMidi {
        info,
        document: Some(std::sync::Arc::new(document)),
    })
}

/// 从磁盘加载 Lumino 工程，自动识别文件夹、新归档或旧版 LMPJ。
pub fn load_project(path: impl AsRef<Path>) -> crate::ExportResult<LuminoProject> {
    let path = path.as_ref();

    if path.is_dir() {
        return lumino_core::project::load::load_project(path).map_err(crate::ExportError::from);
    }

    let bytes = std::fs::read(path)?;
    if bytes.len() >= 4 && &bytes[0..4] == b"LMPJ" {
        lumino_core::project::load::load_project(path).map_err(crate::ExportError::from)
    } else {
        load_legacy_lmpj(&bytes)
    }
}

/// 加载旧版 LMPJ 文件（bincode + zstd），仅保留基本信息。
fn load_legacy_lmpj(bytes: &[u8]) -> crate::ExportResult<LuminoProject> {
    let lmpj_data: lumino_midi_loader::LmpjData = crate::format::decode_lmpj(bytes)?;
    let parsed = lmpj_data.to_parsed_midi();

    let mut project = LuminoProject::new(
        parsed
            .info
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into()),
    );

    // 填充基本信息
    project.metadata.audio.track_count = parsed.info.track_count;
    project.metadata.audio.total_notes = parsed.info.total_notes;
    project.metadata.audio.total_ticks = parsed.info.duration_ticks;
    project.metadata.audio.division = parsed.info.division;

    // 旧版 LMPJ 不包含分轨数据，需要重新解析 MIDI 才能获取
    // 这里仅创建占位符
    for track_id in 0..parsed.info.track_count {
        project.tracks.push(TrackSlot::Unloaded {
            track_id,
            path: PathBuf::new(),
        });
    }

    Ok(project)
}

impl From<lumino_core::CoreError> for crate::ExportError {
    fn from(err: lumino_core::CoreError) -> Self {
        match err {
            lumino_core::CoreError::Io(e) => crate::ExportError::Io(e),
            lumino_core::CoreError::Serialization(s) => crate::ExportError::Encoding(s),
            lumino_core::CoreError::Compression(s) => crate::ExportError::Compression(s),
            lumino_core::CoreError::FileFormat(s) => crate::ExportError::FileFormat(s),
            lumino_core::CoreError::MidiParse(s) => crate::ExportError::MidiParse(s),
            lumino_core::CoreError::InvalidArgument(s) => crate::ExportError::InvalidData(s),
            _ => crate::ExportError::Encoding(err.to_string()),
        }
    }
}
