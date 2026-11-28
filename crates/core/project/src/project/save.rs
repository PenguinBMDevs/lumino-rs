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
    // 创建目录结构（含空目录 data/image、data/loaded/*）
    folder::create_folder_structure(base)?;
    for part in build_parts(project)? {
        let path = base.join(&part.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &part.bytes)?;
    }
    Ok(())
}

/// 保存为单文件归档形态
///
/// DEBT-01 #118：同卷临时文件 → `sync_all` → 换入目标；写失败时原文件保持
/// 不变（Windows 上先旧→`.bak` 再换入，失败自动回滚）。
pub fn save_to_archive(project: &LuminoProject, path: impl AsRef<Path>) -> Result<()> {
    let archive_bytes = encode_archive(project)?;
    atomic::write_file_atomic(path.as_ref(), &archive_bytes)?;
    Ok(())
}

/// 把工程编码为单文件归档字节（**不落盘**，归档头写入当前时间）
///
/// 与 [`save_to_archive`] 同源，供"只要字节、不要文件"的场景直接使用
/// （例：协作房间工程上传），避免「写临时文件 → 读回 → 删除」的往返。
///
/// ⚠️ 归档头与 `metadata.toml` 含**编码时刻**的时间戳（`SystemTime::now()` /
/// `chrono::Local::now()`），所以同一份数据两次编码的字节不一定相同。
/// 需要**逐字节可复现**时用 [`encode_archive_with_time`]，并把 metadata 的
/// 时间字段一并固定（本函数不做归一化）。
pub fn encode_archive(project: &LuminoProject) -> Result<Vec<u8>> {
    let files = build_archive_files(project)?;
    archive::build_archive(&files)
}

/// 把工程编码为单文件归档字节（**确定性入口**：归档头时间由调用方给定）
///
/// `created_at` 是归档里唯一的编码时刻字段；把它固定后，
/// 「相同工程（含相同 metadata 时间戳）+ 相同 `created_at` ⇒ 逐字节相同」。
/// 注意 `metadata.toml` 的 `created_at` / `modified_at` 仍取自 `project.metadata`，
/// 内容指纹场景需由调用方把它们归一化（见协作房间工程）。
pub fn encode_archive_with_time(project: &LuminoProject, created_at: u64) -> Result<Vec<u8>> {
    let files = build_archive_files(project)?;
    archive::build_archive_with_time(&files, created_at)
}

/// 一个工程部件：相对路径 + 已编码字节 + 归档是否再压缩。
struct Part {
    path: String,
    bytes: Vec<u8>,
    /// 归档容器是否对字节再做一层 zstd；文件夹形态忽略此标志。
    compress: bool,
}

impl Part {
    fn new(path: impl Into<String>, bytes: Vec<u8>, compress: bool) -> Self {
        Self {
            path: path.into(),
            bytes,
            compress,
        }
    }
}

/// 构建工程的全部部件（**单一清单**：新增部件只改这里）。
///
/// 文件夹形态与归档形态共用本清单，差异只在容器：
/// - 文件夹：按 `path` 落盘，忽略 `compress`；
/// - 归档：交给 `archive::build_archive`，按 `compress` 决定是否再压一层。
///
/// 顺序即归档文件表顺序（保持历史顺序，避免归档字节无谓变化）。
fn build_parts(project: &LuminoProject) -> Result<Vec<Part>> {
    let mut parts = Vec::new();

    // metadata.toml
    let metadata = build_metadata(project);
    parts.push(Part::new(
        folder::FolderPaths::METADATA_FILE,
        metadata.to_toml_str()?.into_bytes(),
        true,
    ));

    // version
    parts.push(Part::new(
        folder::FolderPaths::VERSION_FILE,
        b"1".to_vec(),
        false,
    ));

    // 音轨
    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        let path = format!("{}/{:03}.lmtrack", folder::FolderPaths::TRACKS_DIR, idx);
        parts.push(Part::new(path, data.encode()?, true));
    }

    push_event_parts(project, &mut parts)?;
    push_text_parts(project, &mut parts)?;

    // 形状（专用格式 LMSH；空形状不写部件 ⇒ 老工程保存后字节不变）
    if let Some(encoded) = encode_shapes(project)? {
        parts.push(Part::new(folder::FolderPaths::SHAPES_FILE, encoded, true));
    }

    Ok(parts)
}

/// 追加时序 / 控制 / 触后事件表部件（LMTM · LMSG · LMCT · LMAT）
///
/// 只负责往清单尾部追加，顺序与 [`push_text_parts`] 的调用先后共同构成归档文件表顺序。
fn push_event_parts(project: &LuminoProject, parts: &mut Vec<Part>) -> Result<()> {
    // tempo（专用格式 LMTM）
    let tempo_data = LmtempData {
        tempo_changes: project.tempo_changes.clone(),
        default_bpm: project.metadata.audio.default_bpm as f32,
    };
    parts.push(Part::new(
        folder::FolderPaths::TEMPO_FILE,
        tempo_data.encode()?,
        true,
    ));

    // signature（专用格式 LMSG）
    let sig_data = LmsigData {
        time_signatures: project.time_signatures.clone(),
        key_signatures: project.key_signatures.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::SIGNATURE_FILE,
        sig_data.encode()?,
        true,
    ));

    // controls（专用格式 LMCT）
    let ctl_data = LmctlData {
        control_changes: project.control_changes.clone(),
        program_changes: project.program_changes.clone(),
        pitch_bends: project.pitch_bends.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::CONTROLS_FILE,
        ctl_data.encode()?,
        true,
    ));

    // aftertouch（专用格式 LMAT，与 LMCT 独立文件，老工程兼容）
    let aftertouch_data = LmcatData {
        channel_aftertouch: project.channel_aftertouch.clone(),
        poly_aftertouch: project.poly_aftertouch.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::AFTERTOUCH_FILE,
        aftertouch_data.encode()?,
        true,
    ));

    Ok(())
}

/// 追加文本 / 系统专属 / 音轨名部件（LMTX · LMMT · LMSY · LMNM）
///
/// 与 [`push_event_parts`] 分开只为让清单可读；两者合起来仍是同一份历史顺序。
fn push_text_parts(project: &LuminoProject, parts: &mut Vec<Part>) -> Result<()> {
    // text events（专用格式 LMTX）
    let txt_data = LmtxtData {
        lyrics: project.lyrics.clone(),
        markers: project.markers.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::TEXT_EVENTS_FILE,
        txt_data.encode()?,
        true,
    ));

    // text metas（专用格式 LMMT，与 LMTX 独立文件，老工程兼容）
    let txtmeta_data = LmtextmetaData {
        text_events: project.text_events.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::TEXT_METAS_FILE,
        txtmeta_data.encode()?,
        true,
    ));

    // sysex（专用格式 LMSY）
    let syx_data = LmsyxData {
        sys_ex: project.sys_ex.clone(),
    };
    parts.push(Part::new(
        folder::FolderPaths::SYSEX_FILE,
        syx_data.encode()?,
        true,
    ));

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
    parts.push(Part::new(
        folder::FolderPaths::TRACK_NAMES_FILE,
        names_data.encode()?,
        true,
    ));

    Ok(())
}

/// 归档文件列表（`build_parts` 的容器投影）。
fn build_archive_files(project: &LuminoProject) -> Result<Vec<(String, Vec<u8>, bool)>> {
    Ok(build_parts(project)?
        .into_iter()
        .map(|part| (part.path, part.bytes, part.compress))
        .collect())
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
