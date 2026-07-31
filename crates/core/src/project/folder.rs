//! 文件夹形态工程读写
//!
//! `.lmpj` 包文件夹本质上是一个按约定结构组织的目录。

use std::path::{Path, PathBuf};

use crate::project::track::LmtrackData;
use crate::{CoreError, Result};

/// 文件夹形态工程路径常量
pub struct FolderPaths;

impl FolderPaths {
    /// Lumino 内部目录
    pub const LUMINO_DIR: &str = ".lumino";
    /// 版本文件
    pub const VERSION_FILE: &str = ".lumino/version";
    /// 元数据文件
    pub const METADATA_FILE: &str = "metadata.toml";
    /// 数据目录
    pub const DATA_DIR: &str = "data";
    /// 工程数据目录
    pub const PROJECT_DIR: &str = "data/project";
    /// 音轨数据目录
    pub const TRACKS_DIR: &str = "data/project/tracks";
    /// 图标目录
    pub const IMAGE_DIR: &str = "data/image";
    /// 导入数据目录
    pub const LOADED_DIR: &str = "data/loaded";
    /// 速度文件
    pub const TEMPO_FILE: &str = "data/project/tempo.lmtemp";
    /// 拍号文件
    pub const SIGNATURE_FILE: &str = "data/project/signature.lmsig";
    /// 控制事件文件
    pub const CONTROLS_FILE: &str = "data/project/controls.lmctl";
    /// 文本 meta 事件文件（歌词 + 标记）
    pub const TEXT_EVENTS_FILE: &str = "data/project/text_events.lmtxt";
    /// SysEx 事件文件
    pub const SYSEX_FILE: &str = "data/project/sysex.lmsyx";
    /// 音轨名称文件
    pub const TRACK_NAMES_FILE: &str = "data/project/track_names.lmnames";
}

/// 创建文件夹工程的基础目录结构
pub fn create_folder_structure(base: impl AsRef<Path>) -> Result<()> {
    let base = base.as_ref();
    std::fs::create_dir_all(base.join(FolderPaths::LUMINO_DIR))?;
    std::fs::create_dir_all(base.join(FolderPaths::TRACKS_DIR))?;
    std::fs::create_dir_all(base.join(FolderPaths::IMAGE_DIR))?;
    std::fs::create_dir_all(base.join(FolderPaths::LOADED_DIR))?;
    std::fs::create_dir_all(base.join(FolderPaths::LOADED_DIR).join("mid"))?;
    std::fs::create_dir_all(base.join(FolderPaths::LOADED_DIR).join("lmpj"))?;
    Ok(())
}

/// 写入版本文件
pub fn write_version_file(base: impl AsRef<Path>, version: u16) -> Result<()> {
    let path = base.as_ref().join(FolderPaths::VERSION_FILE);
    std::fs::write(path, version.to_string())?;
    Ok(())
}

/// 读取版本文件
pub fn read_version_file(base: impl AsRef<Path>) -> Result<u16> {
    let path = base.as_ref().join(FolderPaths::VERSION_FILE);
    let content = std::fs::read_to_string(path)?;
    content
        .trim()
        .parse()
        .map_err(|e| CoreError::FileFormat(format!("version file: {e}")))
}

/// 读取所有音轨文件
pub fn read_all_tracks(base: impl AsRef<Path>) -> Result<Vec<LmtrackData>> {
    let tracks_dir = base.as_ref().join(FolderPaths::TRACKS_DIR);
    let mut tracks = Vec::new();

    if !tracks_dir.exists() {
        return Ok(tracks);
    }

    let mut entries: Vec<_> = std::fs::read_dir(&tracks_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "lmtrack")
                .unwrap_or(false)
        })
        .collect();

    // 按文件名排序，保证顺序
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let bytes = std::fs::read(&path)?;
        let track = LmtrackData::decode(&bytes)?;
        tracks.push(track);
    }

    Ok(tracks)
}

/// 写入音轨文件
pub fn write_track(base: impl AsRef<Path>, track_id: u16, data: &LmtrackData) -> Result<()> {
    let tracks_dir = base.as_ref().join(FolderPaths::TRACKS_DIR);
    std::fs::create_dir_all(&tracks_dir)?;

    let filename = format!("{:03}.lmtrack", track_id);
    let path = tracks_dir.join(filename);
    let encoded = data.encode()?;
    std::fs::write(path, encoded)?;
    Ok(())
}

/// 获取音轨文件路径
pub fn track_path(base: impl AsRef<Path>, track_id: u16) -> PathBuf {
    base.as_ref()
        .join(FolderPaths::TRACKS_DIR)
        .join(format!("{:03}.lmtrack", track_id))
}

/// 通用二进制数据文件编码（带魔数 + bincode + zstd）
pub fn encode_binary_file(
    magic: &[u8; 4],
    version: u16,
    data: &impl serde::Serialize,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    result.extend_from_slice(magic);
    result.extend_from_slice(&version.to_le_bytes());

    let serialized = bincode::serialize(data).map_err(CoreError::from)?;
    let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3)
        .map_err(|e| CoreError::Compression(format!("zstd: {e}")))?;

    result.extend_from_slice(&compressed);
    Ok(result)
}

/// 通用二进制数据文件解码（验证魔数 + zstd + bincode）
pub fn decode_binary_file<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    expected_magic: &[u8; 4],
) -> Result<T> {
    if bytes.len() < 6 {
        return Err(CoreError::FileFormat("binary file: too short".into()));
    }
    if &bytes[0..4] != expected_magic {
        return Err(CoreError::FileFormat("binary file: invalid magic".into()));
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != 1 {
        return Err(CoreError::FileFormat(format!(
            "binary file: unsupported version {version}"
        )));
    }

    let decompressed = zstd::stream::decode_all(std::io::Cursor::new(&bytes[6..]))
        .map_err(|e| CoreError::Compression(format!("decompression: {e}")))?;

    bincode::deserialize(&decompressed).map_err(CoreError::from)
}

#[cfg(test)]
mod tests {
    use super::super::track::{TrackMeta, TrackVisibilitySer};
    use super::*;

    #[test]
    fn test_folder_structure_creation() {
        let test_dir = std::env::temp_dir().join("lumino_test_folder");
        let _ = std::fs::remove_dir_all(&test_dir);

        create_folder_structure(&test_dir).expect("创建文件夹结构失败");
        assert!(test_dir.join(FolderPaths::LUMINO_DIR).exists());
        assert!(test_dir.join(FolderPaths::TRACKS_DIR).exists());
        assert!(test_dir.join(FolderPaths::IMAGE_DIR).exists());
        assert!(test_dir.join(FolderPaths::LOADED_DIR).join("mid").exists());

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_version_file_roundtrip() {
        let version_dir = std::env::temp_dir().join("lumino_test_version");
        let _ = std::fs::remove_dir_all(&version_dir);
        std::fs::create_dir_all(version_dir.join(FolderPaths::LUMINO_DIR))
            .expect("创建.lumino目录失败");

        write_version_file(&version_dir, 1).expect("写入版本文件失败");
        let version = read_version_file(&version_dir).expect("读取版本文件失败");
        assert_eq!(version, 1);

        let _ = std::fs::remove_dir_all(&version_dir);
    }

    #[test]
    fn test_write_and_read_track() {
        let track_dir = std::env::temp_dir().join("lumino_test_track");
        let _ = std::fs::remove_dir_all(&track_dir);
        std::fs::create_dir_all(&track_dir).expect("创建临时目录失败");

        let meta = TrackMeta {
            track_id: 0,
            name: "Test".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 100,
        };
        let data = LmtrackData::from_compact_events(
            meta,
            &[lumino_midi_model::compact::CompactEvent::new(
                0,
                0,
                lumino_midi_model::compact::EventKind::NoteOn,
                0,
                60,
                100,
            )],
        );

        write_track(&track_dir, 0, &data).expect("写入轨道数据失败");
        let tracks = read_all_tracks(&track_dir).expect("读取所有轨道数据失败");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].event_count, 1);

        let _ = std::fs::remove_dir_all(&track_dir);
    }
}
