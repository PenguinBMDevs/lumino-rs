//! 工程加载逻辑
//!
//! 从文件夹形态或单文件形态加载为内存中的 `LuminoProject`。
//!
//! 旧版 LMPJ 兼容加载保留在 `lumino-export` 中，避免核心 crate 依赖加载器。

mod archive;
mod folder;

use std::path::Path;

use crate::LuminoProject;
use lumino_core::error::{CoreError, Result};

use self::{archive::load_from_archive, folder::load_from_folder};

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
