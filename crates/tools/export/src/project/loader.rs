//! 工程文件加载逻辑（文件夹 / 新归档 / 入口文件 / 旧版 LMPJ 自动识别）

use std::path::{Path, PathBuf};

use lumino_midi_loader::LmpjData;
use lumino_project::project::{LuminoProject, TrackSlot};

use super::entry::try_parse_entry_file;
use crate::format::decode_lmpj;
use crate::{ExportError, ExportResult};

/// 从磁盘加载 Lumino 工程，自动识别文件夹、新归档、入口文件或旧版 LMPJ。
pub fn load_project(path: impl AsRef<Path>) -> ExportResult<LuminoProject> {
    let path = path.as_ref();

    if path.is_dir() {
        return lumino_project::project::load::load_project(path).map_err(ExportError::from);
    }

    let bytes = std::fs::read(path)?;
    if bytes.len() >= 4 && &bytes[0..4] == b"LMPJ" {
        return lumino_project::project::load::load_project(path).map_err(ExportError::from);
    }

    // 尝试作为入口文件解析
    if let Ok(text) = std::str::from_utf8(&bytes)
        && let Some(entry) = try_parse_entry_file(text)
    {
        let data_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&entry.data_folder);
        if data_dir.is_dir() {
            return lumino_project::project::load::load_project(&data_dir)
                .map_err(ExportError::from);
        }
        return Err(ExportError::FileFormat(format!(
            "入口文件指向的数据文件夹不存在: {:?}",
            data_dir
        )));
    }

    load_legacy_lmpj(&bytes)
}

/// 加载旧版 LMPJ 文件（bincode + zstd），仅保留基本信息
fn load_legacy_lmpj(bytes: &[u8]) -> ExportResult<LuminoProject> {
    let lmpj_data: LmpjData = decode_lmpj(bytes)?;
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

    // 旧版 LMPJ 不包含分轨数据，创建占位符
    for track_id in 0..parsed.info.track_count {
        project.tracks.push(TrackSlot::Unloaded {
            track_id,
            path: PathBuf::new(),
        });
    }

    Ok(project)
}
