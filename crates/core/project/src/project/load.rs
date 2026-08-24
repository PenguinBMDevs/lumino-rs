//! 工程加载逻辑
//!
//! 从文件夹形态或单文件形态加载为内存中的 `LuminoProject`。
//!
//! 旧版 LMPJ 兼容加载保留在 `lumino-export` 中，避免核心 crate 依赖加载器。

use std::path::{Path, PathBuf};

use crate::project::{
    archive,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmsyxData, LmtempData, LmtxtData},
    folder,
    metadata::ProjectMetadata,
};
use crate::{LmtrackData, LuminoProject, TrackSlot};
use lumino_core::error::{CoreError, Result};

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

/// 从归档字节加载工程（内存加载，用于编译期嵌入的素材文件）
///
/// 校验 LMPJ 魔数后直接走 `load_from_archive` 解析路径，
/// 供嵌入式素材（include_bytes! 数据）在运行时解析使用。
pub fn load_project_from_bytes(bytes: &[u8]) -> Result<LuminoProject> {
    if bytes.len() < 4 || &bytes[0..4] != b"LMPJ" {
        return Err(CoreError::FileFormat(
            "不是有效的 Lumino 工程归档（缺少 LMPJ 魔数）".into(),
        ));
    }
    load_from_archive(bytes)
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
        project.pitch_bends = data.pitch_bends;
    }

    // 读取 text events（专用格式 LMTX）
    if let Some(txt_bytes) =
        archive::read_file_from_archive(bytes, "data/project/text_events.lmtxt")
            .map_err(|e| CoreError::FileFormat(format!("读取 text events 失败: {e}")))?
    {
        let data = LmtxtData::decode(&txt_bytes)
            .map_err(|e| CoreError::FileFormat(format!("text events 解码失败: {e}")))?;
        project.lyrics = data.lyrics;
        project.markers = data.markers;
    }

    // 读取 SysEx（专用格式 LMSY）
    if let Some(syx_bytes) = archive::read_file_from_archive(bytes, "data/project/sysex.lmsyx")
        .map_err(|e| CoreError::FileFormat(format!("读取 SysEx 失败: {e}")))?
    {
        let data = LmsyxData::decode(&syx_bytes)
            .map_err(|e| CoreError::FileFormat(format!("SysEx 解码失败: {e}")))?;
        project.sys_ex = data.sys_ex;
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
    use crate::{LmtrackData, TrackMeta, TrackVisibilitySer};
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
        let load_dir = std::env::temp_dir().join("lumino_load_folder_test");
        let _ = std::fs::remove_dir_all(&load_dir);

        crate::project::save::save_to_folder(&project, &load_dir).expect("保存到文件夹失败");
        let loaded = load_from_folder(&load_dir).expect("从文件夹加载项目失败");

        assert_eq!(loaded.metadata.project.name, "Test");
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.loaded_track_count(), 1);

        let _ = std::fs::remove_dir_all(&load_dir);
    }

    #[test]
    fn test_load_from_archive() {
        // 先保存归档再加载
        let project = create_test_project();
        let load_archive_path = std::env::temp_dir().join("lumino_load_archive_test.lmpj");
        let _ = std::fs::remove_file(&load_archive_path);

        crate::project::save::save_to_archive(&project, &load_archive_path)
            .expect("保存到归档失败");
        let bytes = std::fs::read(&load_archive_path).expect("读取归档文件失败");
        let loaded = load_from_archive(&bytes).expect("从归档加载项目失败");

        assert_eq!(loaded.metadata.project.name, "Test");
        assert_eq!(loaded.tracks.len(), 1);

        let _ = std::fs::remove_file(&load_archive_path);
    }

    fn create_event_rich_project() -> LuminoProject {
        let mut project = create_test_project();
        project.tempo_changes = vec![(0, 120.0), (960, 140.0)];
        project.time_signatures = vec![(0, 4, 4), (1920, 3, 4)];
        project.key_signatures = vec![(0, 0, true), (1920, 2, false)];
        project.control_changes = vec![(0, 0, 0, 7, 100), (480, 0, 0, 10, 64)];
        project.program_changes = vec![(0, 0, 0, 1), (960, 0, 0, 5)];
        project.pitch_bends = vec![(240, 0, 0, 2048), (720, 0, 0, -1024)];
        project.lyrics = vec![(0, 0, b"la".to_vec()), (480, 0, b"ti".to_vec())];
        project.markers = vec![(0, 0, b"Intro".to_vec()), (960, 0, b"Chorus".to_vec())];
        project.sys_ex = vec![(240, 0, b"\x01\x02".to_vec())];
        project.metadata.audio.default_bpm = 120.0;
        project
    }

    fn assert_event_rich_project_eq(loaded: &LuminoProject) {
        assert_eq!(loaded.tempo_changes, &[(0, 120.0), (960, 140.0)]);
        assert_eq!(loaded.time_signatures, &[(0, 4, 4), (1920, 3, 4)]);
        assert_eq!(loaded.key_signatures, &[(0, 0, true), (1920, 2, false)]);
        assert_eq!(
            loaded.control_changes,
            &[(0, 0, 0, 7, 100), (480, 0, 0, 10, 64)]
        );
        assert_eq!(loaded.program_changes, &[(0, 0, 0, 1), (960, 0, 0, 5)]);
        assert_eq!(loaded.pitch_bends, &[(240, 0, 0, 2048), (720, 0, 0, -1024)]);
        assert_eq!(
            loaded.lyrics,
            &[(0, 0, b"la".to_vec()), (480, 0, b"ti".to_vec())]
        );
        assert_eq!(
            loaded.markers,
            &[(0, 0, b"Intro".to_vec()), (960, 0, b"Chorus".to_vec())]
        );
        assert_eq!(loaded.sys_ex, &[(240, 0, b"\x01\x02".to_vec())]);
        assert!((loaded.metadata.audio.default_bpm - 120.0).abs() < 0.001);
    }

    #[test]
    fn test_load_events_roundtrip_folder() {
        let project = create_event_rich_project();
        let events_dir = std::env::temp_dir().join("lumino_load_events_folder_test");
        let _ = std::fs::remove_dir_all(&events_dir);

        crate::project::save::save_to_folder(&project, &events_dir).expect("保存到文件夹失败");
        let loaded = load_from_folder(&events_dir).expect("从文件夹加载项目失败");

        assert_event_rich_project_eq(&loaded);

        let _ = std::fs::remove_dir_all(&events_dir);
    }

    #[test]
    fn test_load_events_roundtrip_archive() {
        let project = create_event_rich_project();
        let events_archive_path = std::env::temp_dir().join("lumino_load_events_archive_test.lmpj");
        let _ = std::fs::remove_file(&events_archive_path);

        crate::project::save::save_to_archive(&project, &events_archive_path)
            .expect("保存到归档失败");
        let bytes = std::fs::read(&events_archive_path).expect("读取归档文件失败");
        let loaded = load_from_archive(&bytes).expect("从归档加载项目失败");

        assert_event_rich_project_eq(&loaded);

        let _ = std::fs::remove_file(&events_archive_path);
    }

    /// 回归：作者与版权信息必须随工程文件保存并重新加载后保留
    /// （修复：工程设置面板的作者/版权保存后重新打开显示空白）。
    #[test]
    fn test_author_copyright_survive_save_load() {
        // 归档（单文件 .lmpj）形态
        let mut archive_project = create_test_project();
        archive_project.metadata.project.author = "张三".into();
        archive_project.metadata.project.copyright = "© 2026 Lumino".into();
        let archive_path = std::env::temp_dir().join("lumino_author_copyright_archive_test.lmpj");
        let _ = std::fs::remove_file(&archive_path);
        crate::project::save::save_to_archive(&archive_project, &archive_path)
            .expect("保存归档失败");
        let archive_bytes = std::fs::read(&archive_path).expect("读取归档失败");
        let loaded_archive = load_from_archive(&archive_bytes).expect("从归档加载失败");
        assert_eq!(loaded_archive.metadata.project.author, "张三");
        assert_eq!(loaded_archive.metadata.project.copyright, "© 2026 Lumino");
        let _ = std::fs::remove_file(&archive_path);

        // 文件夹形态（metadata.toml）
        let mut folder_project = create_test_project();
        folder_project.metadata.project.author = "李四".into();
        folder_project.metadata.project.copyright = "© 2026 Lumino".into();
        let folder_dir = std::env::temp_dir().join("lumino_author_copyright_folder_test");
        let _ = std::fs::remove_dir_all(&folder_dir);
        crate::project::save::save_to_folder(&folder_project, &folder_dir).expect("保存文件夹失败");
        let loaded_folder = load_from_folder(&folder_dir).expect("从文件夹加载失败");
        assert_eq!(loaded_folder.metadata.project.author, "李四");
        assert_eq!(loaded_folder.metadata.project.copyright, "© 2026 Lumino");
        let _ = std::fs::remove_dir_all(&folder_dir);
    }
}
