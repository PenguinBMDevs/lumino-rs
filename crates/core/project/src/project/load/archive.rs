//! 归档形态工程加载

use std::path::PathBuf;

use crate::project::{archive, metadata::ProjectMetadata};
use crate::{LmtrackData, LuminoProject, TrackSlot};
use lumino_core::error::{CoreError, Result};
use lumino_core::{tr, trf};

/// 从归档文件加载
pub(super) fn load_from_archive(bytes: &[u8]) -> Result<LuminoProject> {
    // DEBT-01 #118：文件头 + 文件表只解析一次（边界/版本/CRC 校验随读取执行）。
    let reader = archive::ArchiveReader::new(bytes)
        .map_err(|e| CoreError::FileFormat(trf!("归档读取失败: {e}", e = e)))?;

    // 读取 metadata.toml
    let meta_bytes = reader
        .read("metadata.toml")
        .map_err(|e| CoreError::FileFormat(trf!("归档读取失败: {e}", e = e)))?
        .ok_or_else(|| CoreError::FileFormat(tr!("归档中缺少 metadata.toml").into()))?;
    let metadata = ProjectMetadata::from_toml_str(
        std::str::from_utf8(&meta_bytes)
            .map_err(|e| CoreError::FileFormat(trf!("metadata 编码错误: {e}", e = e)))?,
    )
    .map_err(|e| CoreError::FileFormat(trf!("metadata 解析失败: {e}", e = e)))?;

    let mut project = LuminoProject::new(&metadata.project.name);
    project.metadata = metadata;

    // 读取音轨（根据 metadata 中的 track_count）
    for track_id in 0..project.metadata.audio.track_count {
        let path = format!("data/project/tracks/{:03}.lmtrack", track_id);
        if let Some(track_bytes) = reader.read(&path).map_err(|e| {
            CoreError::FileFormat(trf!(
                "读取音轨 {track_id} 失败: {e}",
                track_id = track_id,
                e = e
            ))
        })? {
            let track = LmtrackData::decode(&track_bytes).map_err(|e| {
                CoreError::FileFormat(trf!(
                    "解码音轨 {track_id} 失败: {e}",
                    track_id = track_id,
                    e = e
                ))
            })?;
            let idx = track_id as usize;
            if idx >= project.tracks.len() {
                project.tracks.resize_with(idx + 1, || TrackSlot::Unloaded {
                    track_id: 0,
                    path: PathBuf::new(),
                });
            }
            project.tracks[idx] = TrackSlot::Loaded(track);
        }
    }

    // 事件/元数据部件（9 段解码与 folder / archive 共用）
    super::events::apply_event_parts(&mut project, |part: &str| {
        reader
            .read(part)
            .map_err(|e| CoreError::FileFormat(trf!("读取 {part} 失败: {e}", part = part, e = e)))
    })?;

    Ok(project)
}
