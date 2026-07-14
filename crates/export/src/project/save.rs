//! 工程保存逻辑
//!
//! 将内存中的 `LuminoProject` 保存为文件夹形态或单文件形态。

use std::path::Path;

use crate::project::{
    LuminoProject, TrackSlot, archive,
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    folder,
    metadata::{LoadedFileMetadataEntry, LoadedMetadata, ProjectMetadata, TrackMetadataEntry},
};

/// 保存为文件夹形态
pub fn save_to_folder(project: &LuminoProject, path: impl AsRef<Path>) -> crate::ExportResult<()> {
    let base = path.as_ref();

    // 创建目录结构
    folder::create_folder_structure(base)
        .map_err(|e| crate::ExportError::Io(std::io::Error::other(e)))?;

    // 写入版本文件
    folder::write_version_file(base, 1)
        .map_err(|e| crate::ExportError::Io(std::io::Error::other(e)))?;

    // 更新并写入 metadata.toml
    let metadata = build_metadata(project);
    metadata
        .to_file(base.join(folder::FolderPaths::METADATA_FILE))
        .map_err(|e| crate::ExportError::Io(std::io::Error::other(e)))?;

    // 写入音轨
    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        folder::write_track(base, idx as u16, data)
            .map_err(|e| crate::ExportError::Io(std::io::Error::other(e)))?;
    }

    // 写入 tempo 数据（专用格式 LMTM）
    let tempo_data = LmtempData {
        tempo_changes: project.tempo_changes.clone(),
        default_bpm: project.metadata.audio.default_bpm as f32,
    };
    let encoded = tempo_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("tempo encode: {e}")))?;
    std::fs::write(base.join(folder::FolderPaths::TEMPO_FILE), encoded)?;

    // 写入 signature 数据（专用格式 LMSG）
    let sig_data = LmsigData {
        time_signatures: project.time_signatures.clone(),
        key_signatures: project.key_signatures.clone(),
    };
    let encoded = sig_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("signature encode: {e}")))?;
    std::fs::write(base.join(folder::FolderPaths::SIGNATURE_FILE), encoded)?;

    // 写入 control 数据（专用格式 LMCT）
    let ctl_data = LmctlData {
        control_changes: project.control_changes.clone(),
        program_changes: project.program_changes.clone(),
        pitch_bends: Vec::new(), // TODO: 从 MidiDocument 提取弯音事件
    };
    let encoded = ctl_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("controls encode: {e}")))?;
    std::fs::write(base.join(folder::FolderPaths::CONTROLS_FILE), encoded)?;

    // 写入音轨名称映射表（专用格式 LMNM）
    let names_data = LmnamesData {
        track_names: project
            .tracks
            .iter()
            .map(|slot| match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some(d.meta.name.clone()),
                TrackSlot::Unloaded { .. } => None,
            })
            .collect(),
    };
    let encoded = names_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("names encode: {e}")))?;
    std::fs::write(base.join(folder::FolderPaths::TRACK_NAMES_FILE), encoded)?;

    Ok(())
}

/// 保存为单文件归档形态
pub fn save_to_archive(project: &LuminoProject, path: impl AsRef<Path>) -> crate::ExportResult<()> {
    let files = build_archive_files(project)?;
    let archive_bytes = archive::build_archive(&files)
        .map_err(|e| crate::ExportError::Encoding(format!("构建归档失败: {e}")))?;
    std::fs::write(path, archive_bytes)?;
    Ok(())
}

/// 构建归档文件列表
fn build_archive_files(
    project: &LuminoProject,
) -> crate::ExportResult<Vec<(String, Vec<u8>, bool)>> {
    let mut files: Vec<(String, Vec<u8>, bool)> = Vec::new();

    // metadata.toml
    let metadata = build_metadata(project);
    let meta_str = metadata
        .to_toml_str()
        .map_err(|e| crate::ExportError::Encoding(format!("metadata encode: {e}")))?;
    files.push(("metadata.toml".into(), meta_str.into_bytes(), true));

    // version
    files.push((".lumino/version".into(), b"1".to_vec(), false));

    // 音轨
    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        let encoded = data
            .encode()
            .map_err(|e| crate::ExportError::Encoding(format!("track encode: {e}")))?;
        let path = format!("data/project/tracks/{:03}.lmtrack", idx);
        files.push((path, encoded, true));
    }

    // tempo（专用格式 LMTM）
    let tempo_data = LmtempData {
        tempo_changes: project.tempo_changes.clone(),
        default_bpm: project.metadata.audio.default_bpm as f32,
    };
    let encoded = tempo_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("tempo encode: {e}")))?;
    files.push(("data/project/tempo.lmtemp".into(), encoded, true));

    // signature（专用格式 LMSG）
    let sig_data = LmsigData {
        time_signatures: project.time_signatures.clone(),
        key_signatures: project.key_signatures.clone(),
    };
    let encoded = sig_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("signature encode: {e}")))?;
    files.push(("data/project/signature.lmsig".into(), encoded, true));

    // controls（专用格式 LMCT）
    let ctl_data = LmctlData {
        control_changes: project.control_changes.clone(),
        program_changes: project.program_changes.clone(),
        pitch_bends: Vec::new(),
    };
    let encoded = ctl_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("controls encode: {e}")))?;
    files.push(("data/project/controls.lmctl".into(), encoded, true));

    // track_names（专用格式 LMNM）
    let names_data = LmnamesData {
        track_names: project
            .tracks
            .iter()
            .map(|slot| match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some(d.meta.name.clone()),
                TrackSlot::Unloaded { .. } => None,
            })
            .collect(),
    };
    let encoded = names_data
        .encode()
        .map_err(|e| crate::ExportError::Encoding(format!("names encode: {e}")))?;
    files.push(("data/project/track_names.lmnames".into(), encoded, true));

    Ok(files)
}

/// 从工程构建元数据
fn build_metadata(project: &LuminoProject) -> ProjectMetadata {
    let mut meta = project.metadata.clone();

    // 更新音频信息
    meta.audio.track_count = project.tracks.len() as u16;
    meta.audio.total_notes = project
        .tracks
        .iter()
        .filter_map(|t| match t {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some(d.note_count),
            TrackSlot::Unloaded { .. } => None,
        })
        .sum();

    // 更新音轨元数据
    meta.tracks.entries = project
        .tracks
        .iter()
        .enumerate()
        .filter_map(|(idx, slot)| {
            let data = match slot {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
                TrackSlot::Unloaded { .. } => return None,
            };
            Some(TrackMetadataEntry {
                track_id: idx as u16,
                name: data.meta.name.clone(),
                channel: data.meta.channel,
                visibility: match data.meta.visibility {
                    crate::project::TrackVisibilitySer::Visible => "visible".into(),
                    crate::project::TrackVisibilitySer::Muted => "muted".into(),
                    crate::project::TrackVisibilitySer::Hidden => "hidden".into(),
                },
                solo: data.meta.solo,
                note_count: data.note_count,
            })
        })
        .collect();

    // 更新导入文件列表
    if !project.loaded_files.is_empty() {
        meta.loaded = Some(LoadedMetadata {
            files: project
                .loaded_files
                .iter()
                .map(|f| LoadedFileMetadataEntry {
                    id: f.id.clone(),
                    original_name: f.original_name.clone(),
                    format: match f.format {
                        crate::project::LoadedFormat::Mid => "mid".into(),
                        crate::project::LoadedFormat::Lmpj => "lmpj".into(),
                    },
                    imported_at: f.imported_at.clone(),
                    storage_path: f.storage_path.to_string_lossy().into_owned(),
                    original_info: None,
                })
                .collect(),
        });
    }

    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::track::{LmtrackData, TrackMeta, TrackVisibilitySer};
    use lumino_midi_model::compact::{CompactEvent, EventKind};

    fn create_test_project() -> LuminoProject {
        let mut project = LuminoProject::new("Test Project");
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
    fn test_save_to_folder() {
        let project = create_test_project();
        let tmp = std::env::temp_dir().join("lumino_save_folder_test");
        let _ = std::fs::remove_dir_all(&tmp);

        save_to_folder(&project, &tmp).expect("保存项目到文件夹失败");

        assert!(tmp.join("metadata.toml").exists());
        assert!(tmp.join(".lumino/version").exists());
        assert!(tmp.join("data/project/tracks/000.lmtrack").exists());
        assert!(tmp.join("data/project/tempo.lmtemp").exists());
        assert!(tmp.join("data/project/signature.lmsig").exists());
        assert!(tmp.join("data/project/controls.lmctl").exists());
        assert!(tmp.join("data/project/track_names.lmnames").exists());

        // 验证魔数
        let tempo_bytes =
            std::fs::read(tmp.join("data/project/tempo.lmtemp")).expect("读取tempo文件失败");
        assert_eq!(&tempo_bytes[0..4], b"LMTM");

        let sig_bytes =
            std::fs::read(tmp.join("data/project/signature.lmsig")).expect("读取signature文件失败");
        assert_eq!(&sig_bytes[0..4], b"LMSG");

        let ctl_bytes =
            std::fs::read(tmp.join("data/project/controls.lmctl")).expect("读取controls文件失败");
        assert_eq!(&ctl_bytes[0..4], b"LMCT");

        let names_bytes = std::fs::read(tmp.join("data/project/track_names.lmnames"))
            .expect("读取track_names文件失败");
        assert_eq!(&names_bytes[0..4], b"LMNM");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_save_to_archive() {
        let project = create_test_project();
        let tmp = std::env::temp_dir().join("lumino_save_archive_test.lmpj");
        let _ = std::fs::remove_file(&tmp);

        save_to_archive(&project, &tmp).expect("保存项目到归档失败");

        assert!(tmp.exists());
        let bytes = std::fs::read(&tmp).expect("读取归档文件失败");
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[0..4], b"LMPJ");

        let _ = std::fs::remove_file(&tmp);
    }
}
