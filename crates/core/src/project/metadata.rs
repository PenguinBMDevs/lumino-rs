//! 工程元数据定义与 TOML 读写
//!
//! `metadata.toml` 存储作品的元信息，兼顾人类可读与机器解析。

use std::path::Path;

/// 音轨元数据条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackMetadataEntry {
    pub track_id: u16,
    pub name: String,
    pub channel: u8,
    pub visibility: String,
    pub solo: bool,
    pub note_count: u64,
}

/// 导入的外部文件条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedFileMetadataEntry {
    pub id: String,
    pub original_name: String,
    pub format: String,
    pub imported_at: String,
    pub storage_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_info: Option<toml::Table>,
}

/// 作品元数据
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectMetadata {
    /// 格式版本
    pub format_version: u32,
    /// 工程信息
    pub project: ProjectInfo,
    /// 音频信息
    pub audio: AudioInfo,
    /// 音轨元数据
    pub tracks: TracksMetadata,
    /// 导入的外部文件
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<LoadedMetadata>,
    /// 工程设置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<SettingsMetadata>,
    /// 统计信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats: Option<StatsMetadata>,
}

/// 工程基本信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub author: String,
    pub created_at: String,
    pub modified_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub lumino_version: String,
}

/// 音频信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioInfo {
    pub division: u16,
    pub total_ticks: u32,
    pub track_count: u16,
    pub total_notes: u64,
    pub default_bpm: f64,
}

/// 音轨元数据集合
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracksMetadata {
    pub entries: Vec<TrackMetadataEntry>,
}

/// 导入文件元数据集合
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedMetadata {
    pub files: Vec<LoadedFileMetadataEntry>,
}

/// 工程设置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SettingsMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synth_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub soundfont_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_scroll_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_256key: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity_filter_threshold: Option<u8>,
}

/// 统计信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatsMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playback_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_time_seconds: Option<u64>,
}

impl ProjectMetadata {
    /// 创建默认元数据
    pub fn default_with_name(name: impl Into<String>) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            format_version: 1,
            project: ProjectInfo {
                name: name.into(),
                author: "Anonymous".into(),
                created_at: now.clone(),
                modified_at: now,
                description: None,
                lumino_version: env!("CARGO_PKG_VERSION").into(),
            },
            audio: AudioInfo {
                division: 480,
                total_ticks: 0,
                track_count: 0,
                total_notes: 0,
                default_bpm: 120.0,
            },
            tracks: TracksMetadata { entries: vec![] },
            loaded: None,
            settings: None,
            stats: None,
        }
    }

    /// 从 TOML 字符串解析
    pub fn from_toml_str(s: &str) -> crate::Result<Self> {
        toml::from_str(s)
            .map_err(|e| crate::CoreError::Serialization(format!("metadata.toml 解析失败: {e}")))
    }

    /// 编码为 TOML 字符串
    pub fn to_toml_str(&self) -> crate::Result<String> {
        toml::to_string_pretty(self)
            .map_err(|e| crate::CoreError::Serialization(format!("metadata.toml 编码失败: {e}")))
    }

    /// 从文件读取
    pub fn from_file(path: impl AsRef<Path>) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(crate::CoreError::Io)?;
        Self::from_toml_str(&content)
    }

    /// 写入文件
    pub fn to_file(&self, path: impl AsRef<Path>) -> crate::Result<()> {
        let content = self.to_toml_str()?;
        std::fs::write(path, content).map_err(crate::CoreError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_roundtrip() {
        let mut meta = ProjectMetadata::default_with_name("Test Project");
        meta.audio.track_count = 2;
        meta.audio.total_notes = 1000;
        meta.tracks.entries.push(TrackMetadataEntry {
            track_id: 0,
            name: "Piano".into(),
            channel: 0,
            visibility: "visible".into(),
            solo: false,
            note_count: 500,
        });

        let toml_str = meta.to_toml_str().unwrap();
        let decoded = ProjectMetadata::from_toml_str(&toml_str).unwrap();

        assert_eq!(decoded.project.name, "Test Project");
        assert_eq!(decoded.audio.track_count, 2);
        assert_eq!(decoded.tracks.entries.len(), 1);
        assert_eq!(decoded.tracks.entries[0].name, "Piano");
    }

    #[test]
    fn test_default_metadata() {
        let meta = ProjectMetadata::default_with_name("Untitled");
        assert_eq!(meta.format_version, 1);
        assert_eq!(meta.project.name, "Untitled");
        assert_eq!(meta.audio.division, 480);
    }
}
