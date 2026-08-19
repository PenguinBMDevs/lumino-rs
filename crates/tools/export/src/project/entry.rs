//! 文件夹工程入口文件（`.lmpj`）的读写与缓存逻辑

use std::path::Path;

use lumino_project::project::metadata::ProjectMetadata;
use lumino_project::project::save::save_to_folder;
use lumino_project::project::{LuminoProject, TrackSlot};

use super::LuminoEntryFile;
use crate::waterfall_export::export_waterfall_tiles;
use crate::{ExportError, ExportResult};

/// 保存工程为文件夹形态，并生成 `.lmpj` 入口文件
///
/// `entry_path`：入口文件路径；数据文件夹为入口去除扩展名后的目录。
pub fn save_project_to_folder_with_entry(
    project: &LuminoProject,
    entry_path: impl AsRef<Path>,
    key_count: u16,
) -> ExportResult<()> {
    let entry_path = entry_path.as_ref();
    let data_folder = entry_path.with_extension("");
    let folder_name = data_folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());

    // 先写入核心数据到数据文件夹
    save_to_folder(project, &data_folder).map_err(ExportError::from)?;

    // 计算项目级缓存哈希，并导出贴图瀑布流到 data/image
    let cache_hash = compute_project_cache_hash(project);
    let image_dir = data_folder.join("data").join("image");
    let image_meta = export_waterfall_tiles(project, &image_dir, &cache_hash, key_count)?;

    // 重新读取 metadata.toml 并追加 image 字段，保持 save_to_folder 已更新的统计信息
    let metadata_path = data_folder.join("metadata.toml");
    let mut metadata = ProjectMetadata::from_file(&metadata_path).map_err(ExportError::from)?;
    metadata.image = Some(image_meta);
    metadata
        .to_file(&metadata_path)
        .map_err(ExportError::from)?;

    // 写入入口文件
    let entry = LuminoEntryFile::folder(folder_name);
    let entry_str =
        toml::to_string_pretty(&entry).map_err(|e| ExportError::Encoding(e.to_string()))?;
    std::fs::write(entry_path, entry_str)?;

    Ok(())
}

/// 计算项目级缓存哈希（xxhash3，工程内容稳定分桶）
fn compute_project_cache_hash(project: &LuminoProject) -> String {
    let mut hasher_input = Vec::new();
    hasher_input.extend_from_slice(project.metadata.project.name.as_bytes());
    hasher_input.extend_from_slice(&project.metadata.audio.division.to_le_bytes());
    hasher_input.extend_from_slice(&project.metadata.audio.total_ticks.to_le_bytes());
    hasher_input.extend_from_slice(&project.metadata.audio.total_notes.to_le_bytes());

    for (idx, slot) in project.tracks.iter().enumerate() {
        let data = match slot {
            TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
            TrackSlot::Unloaded { .. } => continue,
        };
        hasher_input.extend_from_slice(&(idx as u16).to_le_bytes());
        hasher_input.extend_from_slice(&data.meta.channel.to_le_bytes());
        hasher_input.extend_from_slice(&data.note_count.to_le_bytes());
        hasher_input.extend_from_slice(&data.events);
    }

    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(&hasher_input))
}

/// 从 `.lmpj` 入口文件路径读取贴图瀑布流缓存元数据（仅文件夹入口有效）
pub fn load_project_image_metadata(
    entry_path: impl AsRef<Path>,
) -> Option<lumino_project::project::metadata::ImageMetadata> {
    let entry_path = entry_path.as_ref();
    let bytes = std::fs::read(entry_path).ok()?;
    if bytes.len() >= 4 && &bytes[0..4] == b"LMPJ" {
        return None;
    }
    let text = std::str::from_utf8(&bytes).ok()?;
    let entry: LuminoEntryFile = toml::from_str(text).ok()?;
    if entry.version != 1 || entry.format != "folder" {
        return None;
    }
    let data_dir = entry_path.parent()?.join(&entry.data_folder);
    let metadata = ProjectMetadata::from_file(data_dir.join("metadata.toml")).ok()?;
    metadata.image
}

/// 尝试解析入口文件内容
pub(crate) fn try_parse_entry_file(text: &str) -> Option<LuminoEntryFile> {
    let entry: LuminoEntryFile = toml::from_str(text).ok()?;
    (entry.version == 1 && entry.format == "folder").then_some(entry)
}
