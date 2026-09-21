//! 文件夹形态工程加载

use std::path::{Path, PathBuf};

use crate::project::{
    data_formats::{LmctlData, LmnamesData, LmsigData, LmsyxData, LmtempData, LmtxtData},
    folder,
    metadata::ProjectMetadata,
};
use crate::{LuminoProject, TrackSlot};
use lumino_core::error::{CoreError, Result};

/// 从文件夹加载
pub(super) fn load_from_folder(path: &Path) -> Result<LuminoProject> {
    // 读取 metadata.toml
    let metadata = ProjectMetadata::from_file(path.join(folder::FolderPaths::METADATA_FILE))
        .map_err(|e| CoreError::FileFormat(format!("读取 metadata.toml 失败: {e}")))?;

    let mut project = LuminoProject::new(&metadata.project.name);
    project.metadata = metadata;

    // 读取音轨
    let tracks = folder::read_all_tracks(path)
        .map_err(|e| CoreError::FileFormat(format!("读取音轨失败: {e}")))?;
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

    // 读取 tempo（专用格式 LMTM）
    let tempo_path = path.join(folder::FolderPaths::TEMPO_FILE);
    if tempo_path.exists() {
        let bytes = std::fs::read(&tempo_path)?;
        let data = LmtempData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("tempo 解码失败: {e}")))?;
        project.tempo_changes = data.tempo_changes;
    }

    // 读取 signature（专用格式 LMSG）
    let sig_path = path.join(folder::FolderPaths::SIGNATURE_FILE);
    if sig_path.exists() {
        let bytes = std::fs::read(&sig_path)?;
        let data = LmsigData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("signature 解码失败: {e}")))?;
        project.time_signatures = data.time_signatures;
        project.key_signatures = data.key_signatures;
    }

    // 读取 controls（专用格式 LMCT）
    let ctl_path = path.join(folder::FolderPaths::CONTROLS_FILE);
    if ctl_path.exists() {
        let bytes = std::fs::read(&ctl_path)?;
        let data = LmctlData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("controls 解码失败: {e}")))?;
        project.control_changes = data.control_changes;
        project.program_changes = data.program_changes;
        project.pitch_bends = data.pitch_bends;
    }

    // 读取 text events（专用格式 LMTX）
    let txt_path = path.join(folder::FolderPaths::TEXT_EVENTS_FILE);
    if txt_path.exists() {
        let bytes = std::fs::read(&txt_path)?;
        let data = LmtxtData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("text events 解码失败: {e}")))?;
        project.lyrics = data.lyrics;
        project.markers = data.markers;
    }

    // 读取 SysEx（专用格式 LMSY）
    let syx_path = path.join(folder::FolderPaths::SYSEX_FILE);
    if syx_path.exists() {
        let bytes = std::fs::read(&syx_path)?;
        let data = LmsyxData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("SysEx 解码失败: {e}")))?;
        project.sys_ex = data.sys_ex;
    }

    // 读取 track_names（专用格式 LMNM）
    let names_path = path.join(folder::FolderPaths::TRACK_NAMES_FILE);
    if names_path.exists() {
        let bytes = std::fs::read(&names_path)?;
        let _data = LmnamesData::decode(&bytes)
            .map_err(|e| CoreError::FileFormat(format!("names 解码失败: {e}")))?;
        // 名称冗余存储，实际名称从各 .lmtrack 中已读取
    }

    Ok(project)
}
