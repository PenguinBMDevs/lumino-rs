//! 工程加载逻辑
//!
//! 从文件夹形态或单文件形态加载为内存中的 `LuminoProject`。
//!
//! 旧版 LMPJ 兼容加载保留在 `lumino-export` 中，避免核心 crate 依赖加载器。

use std::path::{Path, PathBuf};

use crate::project::{
    LuminoProject, TrackSlot, archive,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    folder,
    metadata::ProjectMetadata,
    track::LmtrackData,
};
use crate::{CoreError, Result};

/// 判断路径是否为新的工程格式（文件夹或以 LMPJ 魔数开头的文件）
pub fn is_project_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    if path.is_dir() {
        return true;
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() >= 4 => &bytes[0..4] == b"LMPJ",
        _ => false,
    }
}

/// 从路径加载工程（仅识别新格式：文件夹或新归档）
pub fn load_project(path: impl AsRef<Path>) -> Result<LuminoProject> {
    let path = path.as_ref();

    if path.is_dir() {
        load_from_folder(path)
    } else {
        let bytes = std::fs::read(path)?;
        if bytes.len() >= 4 && &bytes[0..4] == b"LMPJ" {
            load_from_archive(&bytes)
        } else {
            Err(CoreError::FileFormat(
                "不是有效的 Lumino 工程文件（缺少 LMPJ 魔数）".into(),
            ))
        }
    }
}

/// 从文件夹加载
fn load_from_folder(path: &Path) -> Result<LuminoProject> {
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

/// 从归档文件加载
fn load_from_archive(bytes: &[u8]) -> Result<LuminoProject> {
    // 读取 metadata.toml
    let meta_bytes = archive::read_file_from_archive(bytes, "metadata.toml")
        .map_err(|e| CoreError::FileFormat(format!("归档读取失败: {e}")))?
        .ok_or_else(|| CoreError::FileFormat("归档中缺少 metadata.toml".into()))?;
    let metadata = ProjectMetadata::from_toml_str(
        std::str::from_utf8(&meta_bytes)
            .map_err(|e| CoreError::FileFormat(format!("metadata 编码错误: {e}")))?,
    )
    .map_err(|e| CoreError::FileFormat(format!("metadata 解析失败: {e}")))?;

    let mut project = LuminoProject::new(&metadata.project.name);
    project.metadata = metadata;

    // 读取音轨（根据 metadata 中的 track_count）
    for track_id in 0..project.metadata.audio.track_count {
        let path = format!("data/project/tracks/{:03}.lmtrack", track_id);
        if let Some(track_bytes) = archive::read_file_from_archive(bytes, &path)
            .map_err(|e| CoreError::FileFormat(format!("读取音轨 {track_id} 失败: {e}")))?
        {
            let track = LmtrackData::decode(&track_bytes)
                .map_err(|e| CoreError::FileFormat(format!("解码音轨 {track_id} 失败: {e}")))?;
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

    // 读取 tempo（专用格式 LMTM）
    if let Some(tempo_bytes) =
        archive::read_file_from_archive(bytes, "data/project/tempo.lmtemp")
            .map_err(|e| CoreError::FileFormat(format!("读取 tempo 失败: {e}")))?
    {
        let data = LmtempData::decode(&tempo_bytes)
            .map_err(|e| CoreError::FileFormat(format!("tempo 解码失败: {e}")))?;
        project.tempo_changes = data.tempo_changes;
    }

    // 读取 signature（专用格式 LMSG）
    if let Some(sig_bytes) = archive::read_file_from_archive(bytes, "data/project/signature.lmsig")
        .map_err(|e| CoreError::FileFormat(format!("读取 signature 失败: {e}")))?
    {
        let data = LmsigData::decode(&sig_bytes)
            .map_err(|e| CoreError::FileFormat(format!("signature 解码失败: {e}")))?;
        project.time_signatures = data.time_signatures;
        project.key_signatures = data.key_signatures;
    }

    // 读取 controls（专用格式 LMCT）
    if let Some(ctl_bytes) =
        archive::read_file_from_archive(bytes, "data/project/controls.lmctl")
            .map_err(|e| CoreError::FileFormat(format!("读取 controls 失败: {e}")))?
    {
        let data = LmctlData::decode(&ctl_bytes)
            .map_err(|e| CoreError::FileFormat(format!("controls 解码失败: {e}")))?;
        project.control_changes = data.control_changes;
        project.program_changes = data.program_changes;
    }

    // 读取 track_names（专用格式 LMNM）
    if let Some(names_bytes) =
        archive::read_file_from_archive(bytes, "data/project/track_names.lmnames")
            .map_err(|e| CoreError::FileFormat(format!("读取 names 失败: {e}")))?
    {
        let _data = LmnamesData::decode(&names_bytes)
            .map_err(|e| CoreError::FileFormat(format!("names 解码失败: {e}")))?;
    }

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{LmtrackData, TrackMeta, TrackVisibilitySer};
    use lumino_midi_model::compact::{CompactEvent, EventKind};

    fn create_test_project() -> LuminoProject {
        let mut project = LuminoProject::new("Test");
        let data = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 1000,
            },
            &[
                CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
                CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
            ],
        );
        project.add_track(data);
        project
    }

    #[test]
    fn test_load_from_folder() {
        // 先保存再加载
        let project = create_test_project();
        let tmp = std::env::temp_dir().join("lumino_load_folder_test");
        let _ = std::fs::remove_dir_all(&tmp);

        crate::project::save::save_to_folder(&project, &tmp).expect("保存到文件夹失败");
        let loaded = load_from_folder(&tmp).expect("从文件夹加载项目失败");

        assert_eq!(loaded.metadata.project.name, "Test");
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.loaded_track_count(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_load_from_archive() {
        // 先保存归档再加载
        let project = create_test_project();
        let tmp = std::env::temp_dir().join("lumino_load_archive_test.lmpj");
        let _ = std::fs::remove_file(&tmp);

        crate::project::save::save_to_archive(&project, &tmp).expect("保存到归档失败");
        let bytes = std::fs::read(&tmp).expect("读取归档文件失败");
        let loaded = load_from_archive(&bytes).expect("从归档加载项目失败");

        assert_eq!(loaded.metadata.project.name, "Test");
        assert_eq!(loaded.tracks.len(), 1);

        let _ = std::fs::remove_file(&tmp);
    }
}
