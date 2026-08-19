//! 工程元数据定义与 TOML 读写
//!
//! `metadata.toml` 存储作品的元信息，兼顾人类可读与机器解析。

use std::path::Path;

use lumino_core::error::{CoreError, Result};

/// 音轨元数据条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackMetadataEntry {
    /// 音轨 ID
    pub track_id: u16,
    /// 音轨名称
    pub name: String,
    /// 通道号
    pub channel: u8,
    /// 可见性字符串
    pub visibility: String,
    /// 是否独奏
    pub solo: bool,
    /// 音符数量
    pub note_count: u64,
}

/// 导入的外部文件条目
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedFileMetadataEntry {
    /// 文件条目 ID
    pub id: String,
    /// 原始文件名
    pub original_name: String,
    /// 导入格式字符串
    pub format: String,
    /// 导入时间
    pub imported_at: String,
    /// 存储路径
    pub storage_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 原始附加信息
    pub original_info: Option<toml::Table>,
}

/// 素材元数据（.lmmaterial 素材文件专用）
///
/// 仅素材文件填写本段，标准 `.lmpj` 工程文件省略（向后兼容）。
/// 加载时依据 `is_material` 分辨素材文件与标准工程文件。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MaterialMetadata {
    /// 素材文件标记（true = .lmmaterial 素材文件）
    pub is_material: bool,
    /// 是否多轨素材（true = 多轨，false = 单轨）
    pub multi_track: bool,
    /// 音轨数量（仅多轨素材填写；单轨素材省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_count: Option<u16>,
}

impl MaterialMetadata {
    /// 创建素材标记（按音轨数量自动推导单/多轨形态）
    pub fn for_track_count(track_count: usize) -> Self {
        let multi_track = track_count > 1;
        Self {
            is_material: true,
            multi_track,
            track_count: if multi_track {
                Some(track_count as u16)
            } else {
                None
            },
        }
    }
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
    /// 高精度洋葱皮贴图配置（导出为文件夹时生成）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageMetadata>,
    /// 素材标记（仅 .lmmaterial 素材文件填写；标准工程省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<MaterialMetadata>,
}

/// 工程基本信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectInfo {
    /// 工程名称
    pub name: String,
    /// 作者
    pub author: String,
    /// 创建时间（RFC3339）
    pub created_at: String,
    /// 修改时间（RFC3339）
    pub modified_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 工程描述
    pub description: Option<String>,
    /// 创建该工程的 lumino 版本
    pub lumino_version: String,
}

/// 音频信息
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioInfo {
    /// 每四分音符 tick 数（division）
    pub division: u16,
    /// 工程总 tick 数
    pub total_ticks: u32,
    /// 音轨数量
    pub track_count: u16,
    /// 音符总数
    pub total_notes: u64,
    /// 默认速度（BPM）
    pub default_bpm: f64,
}

/// 音轨元数据集合
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracksMetadata {
    /// 音轨元数据条目列表
    pub entries: Vec<TrackMetadataEntry>,
}

/// 导入文件元数据集合
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoadedMetadata {
    /// 导入文件条目列表
    pub files: Vec<LoadedFileMetadataEntry>,
}

/// 工程设置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SettingsMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 主题名称
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 合成器后端
    pub synth_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 音色库路径
    pub soundfont_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 自动滚动模式
    pub auto_scroll_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 是否启用 256 键位
    pub enable_256key: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 力度过滤阈值
    pub velocity_filter_threshold: Option<u8>,
}

/// 统计信息
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct StatsMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 编辑次数
    pub edit_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 播放次数
    pub playback_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 导出次数
    pub export_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// 累计创作时间（秒）
    pub working_time_seconds: Option<u64>,
}

/// 高精度洋葱皮贴图元数据
///
/// 导出为文件夹时生成 `.lmocache` 贴图缓存到 `data/image`，
/// 加载时根据此配置恢复运行时缓存上下文。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImageMetadata {
    /// 缓存分桶哈希（项目级固定值，避免不同工程缓存冲突）
    pub cache_hash: String,
    /// 单张贴图宽度（像素）
    pub tile_width_px: u32,
    /// 贴图高度对应 key 数量（128 或 256）
    pub key_count: u16,
    /// 每个时间组包含的小节数
    pub measures_per_group: u32,
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
            image: None,
            material: None,
        }
    }

    /// 是否为素材文件（依据 material 段判断，而非文件扩展名）
    pub fn is_material_file(&self) -> bool {
        self.material
            .as_ref()
            .map(|m| m.is_material)
            .unwrap_or(false)
    }

    /// 素材音轨数量（多轨素材返回 track_count，单轨素材返回 1，非素材返回 0）
    pub fn material_track_count(&self) -> usize {
        let Some(material) = &self.material else {
            return 0;
        };
        if material.multi_track {
            material.track_count.map(|c| c as usize).unwrap_or(0)
        } else {
            1
        }
    }

    /// 从 TOML 字符串解析
    pub fn from_toml_str(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(CoreError::from)
    }

    /// 编码为 TOML 字符串
    pub fn to_toml_str(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(CoreError::from)
    }

    /// 从文件读取
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// 写入文件
    pub fn to_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = self.to_toml_str()?;
        std::fs::write(path, content)?;
        Ok(())
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

        let toml_str = meta.to_toml_str().expect("序列化元数据为TOML失败");
        let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("从TOML反序列化元数据失败");

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
        assert!(!meta.is_material_file());
        assert_eq!(meta.material_track_count(), 0);
    }

    #[test]
    fn test_material_metadata_roundtrip() {
        let mut meta = ProjectMetadata::default_with_name("My Material");
        meta.material = Some(MaterialMetadata::for_track_count(4));

        let toml_str = meta.to_toml_str().expect("序列化素材元数据失败");
        let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("反序列化素材元数据失败");

        assert!(decoded.is_material_file());
        assert!(
            matches!(decoded.material, Some(ref m) if m.multi_track && m.track_count == Some(4))
        );
        assert_eq!(decoded.material_track_count(), 4);
    }

    #[test]
    fn test_single_track_material_omits_track_count() {
        let meta = MaterialMetadata::for_track_count(1);
        assert!(meta.is_material);
        assert!(!meta.multi_track);
        assert!(meta.track_count.is_none());

        let mut project = ProjectMetadata::default_with_name("Single");
        project.material = Some(meta);
        assert_eq!(project.material_track_count(), 1);
        // 单轨素材的 [material] 段不序列化 track_count 字段
        // （audio.track_count 是工程统计字段，始终存在，需截取 [material] 段后断言）
        let toml_str = project.to_toml_str().expect("序列化失败");
        let material_section = toml_str
            .split("[material]")
            .nth(1)
            .expect("应包含 [material] 段");
        assert!(!material_section.contains("track_count"));
    }

    #[test]
    fn test_legacy_metadata_without_material_parses() {
        // 旧版工程文件没有 material 段，必须能正常解析（向后兼容）
        let legacy = r#"
format_version = 1

[project]
name = "Legacy"
author = "Anonymous"
created_at = "2026-01-01T00:00:00+00:00"
modified_at = "2026-01-01T00:00:00+00:00"
lumino_version = "0.1.0"

[audio]
division = 480
total_ticks = 1000
track_count = 1
total_notes = 10
default_bpm = 120.0

[tracks]
entries = []
"#;
        let decoded = ProjectMetadata::from_toml_str(legacy).expect("旧版元数据解析失败");
        assert!(!decoded.is_material_file());
        assert!(decoded.material.is_none());
        assert_eq!(decoded.project.name, "Legacy");
    }
}
