//! 文件夹形态工程加载

use std::path::{Path, PathBuf};

use crate::project::{folder, metadata::ProjectMetadata};
use crate::{LuminoProject, TrackSlot};
use lumino_core::error::{CoreError, Result};
use lumino_core::trf;

/// 从文件夹加载
pub(super) fn load_from_folder(path: &Path) -> Result<LuminoProject> {
    // 读取 metadata.toml
    let metadata = ProjectMetadata::from_file(path.join(folder::FolderPaths::METADATA_FILE))
        .map_err(|e| CoreError::FileFormat(trf!("读取 metadata.toml 失败: {e}", e = e)))?;

    let mut project = LuminoProject::new(&metadata.project.name);
    project.metadata = metadata;

    // 读取音轨
    let tracks = folder::read_all_tracks(path)
        .map_err(|e| CoreError::FileFormat(trf!("读取音轨失败: {e}", e = e)))?;
    for track in tracks {
        let track_id = track.meta.track_id;
        let idx = track_id as usize;
        if idx >= project.tracks.len() {
            project.tracks.resize_with(idx + 1, || TrackSlot::Unloaded {
                track_id: 0,
                path: PathBuf::new(),
            });
        }
        project.tracks[idx] = TrackSlot::Loaded(track);
    }

    // 事件/元数据部件（9 段解码与 folder / archive 共用）
    super::events::apply_event_parts(&mut project, |part: &str| -> Result<Option<Vec<u8>>> {
        let full = path.join(part);
        if full.exists() {
            Ok(Some(std::fs::read(full)?))
        } else {
            Ok(None)
        }
    })?;

    Ok(project)
}
