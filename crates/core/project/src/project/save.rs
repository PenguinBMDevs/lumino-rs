//! 工程保存逻辑
//!
//! 将内存中的 `LuminoProject` 保存为文件夹形态或单文件形态。

use std::path::Path;

use crate::{
    LoadedFormat, LuminoProject, TrackSlot, TrackVisibilitySer,
    project::archive,
    project::data_formats::{
        LmcatData, LmctlData, LmnamesData, LmsigData, LmsyxData, LmtempData, LmtextmetaData,
        LmtxtData,
    },
    project::folder,
    project::metadata::{
        LoadedFileMetadataEntry, LoadedMetadata, ProjectMetadata, TrackMetadataEntry,
    },
};
use lumino_core::error::Result;

use super::atomic;

/// 保存为文件夹形态
///
/// DEBT-01 #118：先在同卷临时目录完整写入，成功后再整体换入目标；
/// 中途失败（磁盘满 / 被强杀 / 写入错误）绝不破坏原有工程。
pub fn save_to_folder(project: &LuminoProject, path: impl AsRef<Path>) -> Result<()> {
    let base = path.as_ref().to_path_buf();
    atomic::save_dir_atomic(&base, |tmp| write_folder_contents(project, tmp))
}

/// 在给定目录内完整写入文件夹形态的全部内容。
///
/// 由 [`save_to_folder`] 的原子替换流程调用（参数为临时目录）；
/// 也供需要"直接写入指定目录"的内部场景复用。
fn write_folder_contents(project: &LuminoProject, base: &Path) -> Result<()> {
    // 创建目录结构
    folder::create_folder_structure(base)?;

    // 写入版本文件
    folder::write_version_file(base, 1)?;

    // 更新并写入 metadata.toml
    let metadata = build_metadata(project);
    metadata.to_file(base.join(folder::FolderPaths::METADATA_FILE))?;

    // 写入音轨
    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        folder::write_track(base, idx as u16, data)?;
    }

    // 写入 tempo 数据（专用格式 LMTM）
    let tempo_data = LmtempData {
        tempo_changes: project.tempo_changes.clone(),
        default_bpm: project.metadata.audio.default_bpm as f32,
    };
    let encoded = tempo_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::TEMPO_FILE), encoded)?;

    // 写入 signature 数据（专用格式 LMSG）
    let sig_data = LmsigData {
        time_signatures: project.time_signatures.clone(),
        key_signatures: project.key_signatures.clone(),
    };
    let encoded = sig_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::SIGNATURE_FILE), encoded)?;

    // 写入 control 数据（专用格式 LMCT）
    let ctl_data = LmctlData {
        control_changes: project.control_changes.clone(),
        program_changes: project.program_changes.clone(),
        pitch_bends: project.pitch_bends.clone(),
    };
    let encoded = ctl_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::CONTROLS_FILE), encoded)?;

    // 写入触后数据（专用格式 LMAT，与 LMCT 独立文件，老工程兼容）
    let aftertouch_data = LmcatData {
        channel_aftertouch: project.channel_aftertouch.clone(),
        poly_aftertouch: project.poly_aftertouch.clone(),
    };
    let encoded = aftertouch_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::AFTERTOUCH_FILE), encoded)?;

    // 写入文本 meta 数据（专用格式 LMTX）
    let txt_data = LmtxtData {
        lyrics: project.lyrics.clone(),
        markers: project.markers.clone(),
    };
    let encoded = txt_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::TEXT_EVENTS_FILE), encoded)?;

    // 写入文本类 meta 数据（专用格式 LMMT，与 LMTX 独立文件，老工程兼容）
    let txtmeta_data = LmtextmetaData {
        text_events: project.text_events.clone(),
    };
    let encoded = txtmeta_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::TEXT_METAS_FILE), encoded)?;

    // 写入 SysEx 数据（专用格式 LMSY）
    let syx_data = LmsyxData {
        sys_ex: project.sys_ex.clone(),
    };
    let encoded = syx_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::SYSEX_FILE), encoded)?;

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
    let encoded = names_data.encode()?;
    std::fs::write(base.join(folder::FolderPaths::TRACK_NAMES_FILE), encoded)?;

    Ok(())
}

/// 保存为单文件归档形态
///
/// DEBT-01 #118：同卷临时文件 → `sync_all` → 换入目标；写失败时原文件保持
/// 不变（Windows 上先旧→`.bak` 再换入，失败自动回滚）。
pub fn save_to_archive(project: &LuminoProject, path: impl AsRef<Path>) -> Result<()> {
    let files = build_archive_files(project)?;
    let archive_bytes = archive::build_archive(&files)?;
    atomic::write_file_atomic(path.as_ref(), &archive_bytes)?;
    Ok(())
}

/// 构建归档文件列表
fn build_archive_files(project: &LuminoProject) -> Result<Vec<(String, Vec<u8>, bool)>> {
    let mut files: Vec<(String, Vec<u8>, bool)> = Vec::new();

    // metadata.toml
    let metadata = build_metadata(project);
    let meta_str = metadata.to_toml_str()?;
    files.push(("metadata.toml".into(), meta_str.into_bytes(), true));

    // version
    files.push((".lumino/version".into(), b"1".to_vec(), false));

    // 音轨
    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        let encoded = data.encode()?;
        let path = format!("data/project/tracks/{:03}.lmtrack", idx);
        files.push((path, encoded, true));
    }

    // tempo（专用格式 LMTM）
    let tempo_data = LmtempData {
        tempo_changes: project.tempo_changes.clone(),
        default_bpm: project.metadata.audio.default_bpm as f32,
    };
    let encoded = tempo_data.encode()?;
    files.push(("data/project/tempo.lmtemp".into(), encoded, true));

    // signature（专用格式 LMSG）
    let sig_data = LmsigData {
        time_signatures: project.time_signatures.clone(),
        key_signatures: project.key_signatures.clone(),
    };
    let encoded = sig_data.encode()?;
    files.push(("data/project/signature.lmsig".into(), encoded, true));

    // controls（专用格式 LMCT）
    let ctl_data = LmctlData {
        control_changes: project.control_changes.clone(),
        program_changes: project.program_changes.clone(),
        pitch_bends: project.pitch_bends.clone(),
    };
    let encoded = ctl_data.encode()?;
    files.push(("data/project/controls.lmctl".into(), encoded, true));

    // aftertouch（专用格式 LMAT，与 LMCT 独立文件，老工程兼容）
    let aftertouch_data = LmcatData {
        channel_aftertouch: project.channel_aftertouch.clone(),
        poly_aftertouch: project.poly_aftertouch.clone(),
    };
    let encoded = aftertouch_data.encode()?;
    files.push(("data/project/aftertouch.lmcat".into(), encoded, true));

    // text events（专用格式 LMTX）
    let txt_data = LmtxtData {
        lyrics: project.lyrics.clone(),
        markers: project.markers.clone(),
    };
    let encoded = txt_data.encode()?;
    files.push(("data/project/text_events.lmtxt".into(), encoded, true));

    // text metas（专用格式 LMMT，与 LMTX 独立文件，老工程兼容）
    let txtmeta_data = LmtextmetaData {
        text_events: project.text_events.clone(),
    };
    let encoded = txtmeta_data.encode()?;
    files.push(("data/project/text_metas.lmmtx".into(), encoded, true));

    // sysex（专用格式 LMSY）
    let syx_data = LmsyxData {
        sys_ex: project.sys_ex.clone(),
    };
    let encoded = syx_data.encode()?;
    files.push(("data/project/sysex.lmsyx".into(), encoded, true));

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
    let encoded = names_data.encode()?;
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
                    TrackVisibilitySer::Visible => "visible".into(),
                    TrackVisibilitySer::Muted => "muted".into(),
                    TrackVisibilitySer::Hidden => "hidden".into(),
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
                        LoadedFormat::Mid => "mid".into(),
                        LoadedFormat::Lmpj => "lmpj".into(),
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
    use crate::{LmtrackData, TrackMeta, TrackVisibilitySer};
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
        let save_dir = std::env::temp_dir().join("lumino_save_folder_test");
        let _ = std::fs::remove_dir_all(&save_dir);

        save_to_folder(&project, &save_dir).expect("保存项目到文件夹失败");

        assert!(save_dir.join("metadata.toml").exists());
        assert!(save_dir.join(".lumino/version").exists());
        assert!(save_dir.join("data/project/tracks/000.lmtrack").exists());
        assert!(save_dir.join("data/project/tempo.lmtemp").exists());
        assert!(save_dir.join("data/project/signature.lmsig").exists());
        assert!(save_dir.join("data/project/controls.lmctl").exists());
        assert!(save_dir.join("data/project/text_events.lmtxt").exists());
        assert!(save_dir.join("data/project/sysex.lmsyx").exists());
        assert!(save_dir.join("data/project/track_names.lmnames").exists());

        // 验证魔数
        let tempo_bytes =
            std::fs::read(save_dir.join("data/project/tempo.lmtemp")).expect("读取tempo文件失败");
        assert_eq!(&tempo_bytes[0..4], b"LMTM");

        let sig_bytes = std::fs::read(save_dir.join("data/project/signature.lmsig"))
            .expect("读取signature文件失败");
        assert_eq!(&sig_bytes[0..4], b"LMSG");

        let ctl_bytes = std::fs::read(save_dir.join("data/project/controls.lmctl"))
            .expect("读取controls文件失败");
        assert_eq!(&ctl_bytes[0..4], b"LMCT");

        let txt_bytes = std::fs::read(save_dir.join("data/project/text_events.lmtxt"))
            .expect("读取text_events文件失败");
        assert_eq!(&txt_bytes[0..4], b"LMTX");

        let syx_bytes =
            std::fs::read(save_dir.join("data/project/sysex.lmsyx")).expect("读取sysex文件失败");
        assert_eq!(&syx_bytes[0..4], b"LMSY");

        let names_bytes = std::fs::read(save_dir.join("data/project/track_names.lmnames"))
            .expect("读取track_names文件失败");
        assert_eq!(&names_bytes[0..4], b"LMNM");

        let _ = std::fs::remove_dir_all(&save_dir);
    }

    #[test]
    fn test_save_to_archive() {
        let project = create_test_project();
        let save_archive_path = std::env::temp_dir().join("lumino_save_archive_test.lmpj");
        let _ = std::fs::remove_file(&save_archive_path);

        save_to_archive(&project, &save_archive_path).expect("保存项目到归档失败");

        assert!(save_archive_path.exists());
        let bytes = std::fs::read(&save_archive_path).expect("读取归档文件失败");
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[0..4], b"LMPJ");

        let _ = std::fs::remove_file(&save_archive_path);
    }

    fn atomic_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lumino_save_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建测试目录失败");
        dir
    }

    fn assert_no_leftovers(dir: &std::path::Path) {
        let leftovers: Vec<String> = std::fs::read_dir(dir)
            .expect("读取测试目录失败")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-") || n.ends_with(".bak"))
            .collect();
        assert!(leftovers.is_empty(), "不应残留临时/备份文件: {leftovers:?}");
    }

    /// DEBT-01 #118：覆盖保存必须原子替换——旧目标被整体换掉，不留 tmp/bak。
    #[test]
    fn test_save_to_archive_atomic_replace() {
        let dir = atomic_test_dir("archive_replace");
        let target = dir.join("project.lmpj");
        std::fs::write(&target, b"stale-not-an-archive").expect("写入旧内容失败");

        let project = create_test_project();
        save_to_archive(&project, &target).expect("原子保存归档失败");

        let bytes = std::fs::read(&target).expect("读取归档失败");
        assert_eq!(&bytes[0..4], b"LMPJ", "旧内容必须被完整替换");
        assert!(
            archive::read_file_from_archive(&bytes, "metadata.toml")
                .expect("读取归档内容失败")
                .is_some(),
            "替换后的归档必须可读"
        );
        assert_no_leftovers(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// DEBT-01 #118：文件夹保存整体换入，旧目录的陈旧文件不得残留。
    #[test]
    fn test_save_to_folder_atomic_replace() {
        let dir = atomic_test_dir("folder_replace");
        let target = dir.join("project.lmpj");
        std::fs::create_dir_all(&target).expect("创建旧目录失败");
        std::fs::write(target.join("stale.bin"), b"stale").expect("写入陈旧文件失败");

        let project = create_test_project();
        save_to_folder(&project, &target).expect("原子保存文件夹失败");

        assert!(target.join("metadata.toml").exists());
        assert!(
            !target.join("stale.bin").exists(),
            "旧目录的陈旧文件必须整体消失"
        );
        assert_no_leftovers(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
