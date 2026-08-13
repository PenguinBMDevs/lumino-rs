//! 工程文件格式兼容层
//!
//! 核心类型已迁移到 `lumino-core`，本模块保留旧版 LMPJ 兼容加载、
//! 文件夹工程 `.lmpj` 入口读写与 Runner 便捷扩展
//! （`LuminoProject -> ParsedMidi`）。素材（.lmmaterial）见 `material` 模块。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use lumino_project::project::*;

// 重新导出核心保存函数，保持 `lumino_export::project::save_to_archive` 等路径可用
pub use lumino_project::project::save::{save_to_archive, save_to_folder};

/// 文件夹工程入口文件内容
///
/// `.lmpj` 文件作为入口，指向同目录下的同名数据文件夹。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuminoEntryFile {
    pub version: u32,
    pub format: String,
    pub data_folder: String,
}

impl LuminoEntryFile {
    /// 创建默认文件夹入口
    pub fn folder(data_folder: impl Into<String>) -> Self {
        Self {
            version: 1,
            format: "folder".into(),
            data_folder: data_folder.into(),
        }
    }
}

/// 保存工程为文件夹形态，并生成 `.lmpj` 入口文件
///
/// `entry_path`：入口文件路径；数据文件夹为入口去除扩展名后的目录。
pub fn save_project_to_folder_with_entry(
    project: &LuminoProject,
    entry_path: impl AsRef<Path>,
    key_count: u16,
) -> crate::ExportResult<()> {
    let entry_path = entry_path.as_ref();
    let data_folder = entry_path.with_extension("");
    let folder_name = data_folder
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());

    // 先写入核心数据到数据文件夹
    lumino_project::project::save::save_to_folder(project, &data_folder)
        .map_err(crate::ExportError::from)?;

    // 计算项目级缓存哈希，并导出高精度洋葱皮贴图到 data/image
    let cache_hash = compute_project_cache_hash(project);
    let image_dir = data_folder.join("data").join("image");
    let image_meta = crate::onion_skin_export::export_onion_skin_tiles(
        project,
        &image_dir,
        &cache_hash,
        key_count,
    )?;

    // 重新读取 metadata.toml 并追加 image 字段，保持 save_to_folder 已更新的统计信息
    let metadata_path = data_folder.join("metadata.toml");
    let mut metadata =
        ProjectMetadata::from_file(&metadata_path).map_err(crate::ExportError::from)?;
    metadata.image = Some(image_meta);
    metadata
        .to_file(&metadata_path)
        .map_err(crate::ExportError::from)?;

    // 写入入口文件
    let entry = LuminoEntryFile::folder(folder_name);
    let entry_str =
        toml::to_string_pretty(&entry).map_err(|e| crate::ExportError::Encoding(e.to_string()))?;
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

/// 从 `.lmpj` 入口文件路径读取高精度洋葱皮缓存元数据（仅文件夹入口有效）
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

/// 将 `LuminoProject` 转换为 `ParsedMidi`，供 Runner 复用现有事件流。
pub fn project_to_parsed_midi(
    project: &LuminoProject,
    original_path: impl Into<PathBuf>,
) -> crate::ExportResult<lumino_midi_loader::ParsedMidi> {
    let document = project
        .to_midi_document()
        .map_err(crate::ExportError::from)?;
    let total_notes: u64 = document.notes.iter().map(|v| v.len() as u64).sum();

    let info = lumino_midi_loader::MidiInfo {
        path: original_path.into(),
        track_count: document.track_count,
        total_notes,
        duration_ticks: document.total_ticks,
        division: project.metadata.audio.division,
        parse_progress: Some(100.0),
    };

    Ok(lumino_midi_loader::ParsedMidi {
        info,
        document: Some(std::sync::Arc::new(document)),
    })
}

/// 从磁盘加载 Lumino 工程，自动识别文件夹、新归档、入口文件或旧版 LMPJ。
pub fn load_project(path: impl AsRef<Path>) -> crate::ExportResult<LuminoProject> {
    let path = path.as_ref();

    if path.is_dir() {
        return lumino_project::project::load::load_project(path).map_err(crate::ExportError::from);
    }

    let bytes = std::fs::read(path)?;
    if bytes.len() >= 4 && &bytes[0..4] == b"LMPJ" {
        return lumino_project::project::load::load_project(path).map_err(crate::ExportError::from);
    }

    // 尝试作为入口文件解析
    if let Ok(text) = std::str::from_utf8(&bytes)
        && let Some(entry) = try_parse_entry_file(text)
    {
        let data_dir = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&entry.data_folder);
        if data_dir.is_dir() {
            return lumino_project::project::load::load_project(&data_dir)
                .map_err(crate::ExportError::from);
        }
        return Err(crate::ExportError::FileFormat(format!(
            "入口文件指向的数据文件夹不存在: {:?}",
            data_dir
        )));
    }

    load_legacy_lmpj(&bytes)
}

/// 尝试解析入口文件内容
fn try_parse_entry_file(text: &str) -> Option<LuminoEntryFile> {
    let entry: LuminoEntryFile = toml::from_str(text).ok()?;
    (entry.version == 1 && entry.format == "folder").then_some(entry)
}

/// 加载旧版 LMPJ 文件（bincode + zstd），仅保留基本信息
fn load_legacy_lmpj(bytes: &[u8]) -> crate::ExportResult<LuminoProject> {
    let lmpj_data: lumino_midi_loader::LmpjData = crate::format::decode_lmpj(bytes)?;
    let parsed = lmpj_data.to_parsed_midi();

    let mut project = LuminoProject::new(
        parsed
            .info
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into()),
    );

    // 填充基本信息
    project.metadata.audio.track_count = parsed.info.track_count;
    project.metadata.audio.total_notes = parsed.info.total_notes;
    project.metadata.audio.total_ticks = parsed.info.duration_ticks;
    project.metadata.audio.division = parsed.info.division;

    // 旧版 LMPJ 不包含分轨数据，创建占位符
    for track_id in 0..parsed.info.track_count {
        project.tracks.push(TrackSlot::Unloaded {
            track_id,
            path: PathBuf::new(),
        });
    }

    Ok(project)
}

impl From<lumino_core::CoreError> for crate::ExportError {
    fn from(err: lumino_core::CoreError) -> Self {
        match err {
            lumino_core::CoreError::Io(e) => crate::ExportError::Io(e),
            lumino_core::CoreError::Serialization(s) => crate::ExportError::Encoding(s),
            lumino_core::CoreError::Compression(s) => crate::ExportError::Compression(s),
            lumino_core::CoreError::FileFormat(s) => crate::ExportError::FileFormat(s),
            lumino_core::CoreError::MidiParse(s) => crate::ExportError::MidiParse(s),
            lumino_core::CoreError::InvalidArgument(s) => crate::ExportError::InvalidData(s),
            _ => crate::ExportError::Encoding(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_model::compact::{CompactEvent, EventKind};
    use lumino_project::project::track::{LmtrackData, TrackMeta, TrackVisibilitySer};
    use tempfile::tempdir;

    fn make_test_project() -> LuminoProject {
        let mut project = LuminoProject::new("FolderEntryTest");
        let events = vec![
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
        ];
        let track = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 480,
            },
            &events,
        );
        project.add_track(track);
        project.metadata.audio.total_ticks = 480;
        project.metadata.audio.division = 480;
        project
    }

    #[test]
    fn test_save_and_load_folder_entry() {
        let dir = tempdir().expect("临时目录应创建成功");
        let entry_path = dir.path().join("test_project.lmpj");
        let project = make_test_project();

        save_project_to_folder_with_entry(&project, &entry_path, 128)
            .expect("保存文件夹工程入口失败");

        // 入口文件与数据文件夹应同时存在
        assert!(entry_path.exists(), "入口文件应存在");
        let data_folder = entry_path.with_extension("");
        assert!(data_folder.is_dir(), "数据文件夹应存在");
        assert!(data_folder.join("metadata.toml").exists());
        assert!(data_folder.join("data/project/tracks/000.lmtrack").exists());

        // 加载入口文件应得到相同工程
        let loaded = load_project(&entry_path).expect("加载入口文件失败");
        assert_eq!(loaded.metadata.project.name, "FolderEntryTest");
        assert_eq!(loaded.tracks.len(), 1);
        assert_eq!(loaded.metadata.audio.total_ticks, 480);
    }

    #[test]
    fn test_save_and_load_folder_entry_multi_track_overlapping() {
        let dir = tempdir().expect("临时目录应创建成功");
        let entry_path = dir.path().join("multi_project.lmpj");
        let mut project = LuminoProject::new("MultiTrackOverlap");

        // 音轨 0：单音符
        let track0 = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 0,
                name: "Piano".into(),
                channel: 0,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 480,
            },
            &[
                CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
                CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
            ],
        );
        project.add_track(track0);

        // 音轨 1：重叠音符（曾触发 to_midi_document 的交替 NoteOn/NoteOff 假设）
        let track1 = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 1,
                name: "Synth".into(),
                channel: 1,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 600,
            },
            &[
                // 两个音符重叠：0-480 与 120-600
                CompactEvent::new(0, 1, EventKind::NoteOn, 1, 64, 100),
                CompactEvent::new(120, 1, EventKind::NoteOn, 1, 67, 80),
                CompactEvent::new(360, 1, EventKind::NoteOff, 1, 64, 0),
                CompactEvent::new(120, 1, EventKind::NoteOff, 1, 67, 0),
            ],
        );
        project.add_track(track1);
        project.metadata.audio.total_ticks = 600;
        project.metadata.audio.division = 480;

        save_project_to_folder_with_entry(&project, &entry_path, 128)
            .expect("保存多轨文件夹工程入口失败");

        // 加载后通过 project_to_parsed_midi 重建，这是 Runner 的加载路径
        let loaded = load_project(&entry_path).expect("加载多轨入口文件失败");
        let parsed = project_to_parsed_midi(&loaded, &entry_path).expect("重建 ParsedMidi 失败");

        let document = parsed.document.expect("应包含 MidiDocument");
        assert_eq!(document.track_count(), 2);
        assert_eq!(document.notes[0].len(), 1);
        assert_eq!(document.notes[1].len(), 2);

        // 验证重叠音符被正确重建（ChunkedList 已保证有序，无需再排序）
        let track1_notes = &document.notes[1];
        assert_eq!(track1_notes[0].start_tick, 0);
        assert_eq!(track1_notes[0].end_tick, 480);
        assert_eq!(track1_notes[0].key, 64);
        assert_eq!(track1_notes[1].start_tick, 120);
        assert_eq!(track1_notes[1].end_tick, 600);
        assert_eq!(track1_notes[1].key, 67);
    }

    #[test]
    fn test_load_project_image_metadata() {
        let dir = tempdir().expect("临时目录应创建成功");
        let entry_path = dir.path().join("img_project.lmpj");
        let project = make_test_project();

        save_project_to_folder_with_entry(&project, &entry_path, 128)
            .expect("保存文件夹工程入口失败");

        let meta = load_project_image_metadata(&entry_path).expect("应能读取到 image 元数据");
        assert!(!meta.cache_hash.is_empty());
        assert_eq!(meta.key_count, 128);
        assert_eq!(meta.measures_per_group, 4);
        assert_eq!(meta.tile_width_px, 1920);
    }

    #[test]
    fn test_load_legacy_and_archive_not_image_metadata() {
        let dir = tempdir().expect("临时目录应创建成功");
        let non_entry = dir.path().join("not_entry.lmpj");
        std::fs::write(&non_entry, b"LMPJ\x00\x01").expect("写入测试文件失败");
        assert!(load_project_image_metadata(&non_entry).is_none());
    }

    #[test]
    fn test_load_project_from_plain_folder() {
        let dir = tempdir().expect("临时目录应创建成功");
        let data_folder = dir.path().join("plain_folder");
        let project = make_test_project();
        lumino_project::project::save::save_to_folder(&project, &data_folder)
            .expect("保存到文件夹失败");

        let loaded = load_project(&data_folder).expect("加载普通文件夹失败");
        assert_eq!(loaded.metadata.project.name, "FolderEntryTest");
    }
}
