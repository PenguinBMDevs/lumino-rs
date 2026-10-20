//! 工程保存逻辑
//!
//! 将内存中的 `LuminoProject` 保存为文件夹形态或单文件形态。

use std::path::Path;

use crate::{
    LoadedFormat, LuminoProject, TrackSlot, TrackVisibilitySer,
    project::archive,
    project::data_formats::{
        LmcatData, LmctlData, LmnamesData, LmshapesData, LmsigData, LmsyxData, LmtempData,
        LmtextmetaData, LmtxtData,
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

    // 形状（专用格式 LMSH；空形状不写部件 ⇒ 老工程保存后字节不变）
    if let Some(encoded) = encode_shapes(project)? {
        std::fs::write(base.join(folder::FolderPaths::SHAPES_FILE), encoded)?;
    }

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

    // 形状（专用格式 LMSH；空形状不写部件 ⇒ 老工程归档字节不变）
    if let Some(encoded) = encode_shapes(project)? {
        files.push((folder::FolderPaths::SHAPES_FILE.into(), encoded, true));
    }

    Ok(files)
}

/// 形状部件编码；形状为空时返回 `None`（**缺失即空**，老工程不受影响）
///
/// DRAW-040 / D6：非空时恒写 **v2**。**含 v1 形状部件的工程重新保存后字节会变**
/// （部件版本 1 → 2），这是预期的迁移行为，不是回归；无形状部件的工程仍不写该部件。
fn encode_shapes(project: &LuminoProject) -> Result<Option<Vec<u8>>> {
    if project.shapes.is_empty() {
        return Ok(None);
    }
    let data = LmshapesData {
        shapes: project.shapes.clone(),
    };
    Ok(Some(data.encode()?))
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
mod tests;
