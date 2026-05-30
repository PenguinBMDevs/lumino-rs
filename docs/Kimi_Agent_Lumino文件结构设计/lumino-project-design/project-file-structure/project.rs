//! Lumino 工程文件核心类型定义
//!
//! 定义了 lumino 项目文件结构的核心数据类型，包括：
//! - 工程元数据 (ProjectMetadata)
//! - 音轨数据文件格式 (LmtrackData, LmtrackHeader)
//! - 归档格式 (ArchiveHeader, FileEntry)
//! - 导入数据缓存格式 (LoadedMidiData, LoadedDmsData, LoadedLmpjData)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ───────────────────────────────────────────────
// 工程元数据
// ───────────────────────────────────────────────

/// 工程完整元数据（对应 metadata.toml 的内存表示）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    /// 文件格式版本（当前为 1）
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// 项目基本信息
    #[serde(default)]
    pub project: ProjectInfo,
    /// 音频属性
    #[serde(default)]
    pub audio: AudioInfo,
    /// 音轨列表
    #[serde(default)]
    pub tracks: TrackList,
    /// 导入的外部文件
    #[serde(default)]
    pub loaded: LoadedFileList,
    /// 工程级设置
    #[serde(default)]
    pub settings: ProjectSettings,
    /// 统计信息
    #[serde(default)]
    pub stats: ProjectStats,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            format_version: 1,
            project: ProjectInfo::default(),
            audio: AudioInfo::default(),
            tracks: TrackList::default(),
            loaded: LoadedFileList::default(),
            settings: ProjectSettings::default(),
            stats: ProjectStats::default(),
        }
    }
}

fn default_format_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub created_at: String,
    pub modified_at: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub lumino_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInfo {
    pub division: u16,
    pub total_ticks: u32,
    pub track_count: u16,
    pub total_notes: u64,
    pub default_bpm: f32,
}

impl Default for AudioInfo {
    fn default() -> Self {
        Self {
            division: 480,
            total_ticks: 0,
            track_count: 0,
            total_notes: 0,
            default_bpm: 120.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrackList {
    #[serde(default)]
    pub entries: Vec<TrackEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackEntry {
    pub track_id: u16,
    pub name: String,
    pub channel: u8,
    pub visibility: TrackVisibilitySer,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub note_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackVisibilitySer {
    Visible,
    Muted,
    Hidden,
}

impl Default for TrackVisibilitySer {
    fn default() -> Self {
        Self::Visible
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadedFileList {
    #[serde(default)]
    pub files: Vec<LoadedFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedFileEntry {
    /// SHA-256 前 8 字节 hex
    pub id: String,
    pub original_name: String,
    pub format: LoadedFormat,
    pub imported_at: String,
    pub storage_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoadedFormat {
    Mid,
    Dms,
    Lmpj,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_synth_backend")]
    pub synth_backend: String,
    #[serde(default)]
    pub soundfont_path: String,
    #[serde(default = "default_auto_scroll")]
    pub auto_scroll_mode: String,
    #[serde(default)]
    pub enable_256key: bool,
    #[serde(default = "default_velocity_threshold")]
    pub velocity_filter_threshold: u8,
}

fn default_theme() -> String {
    "Light".into()
}
fn default_synth_backend() -> String {
    "xsynth".into()
}
fn default_auto_scroll() -> String {
    "scrolling".into()
}
fn default_velocity_threshold() -> u8 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectStats {
    #[serde(default)]
    pub edit_count: u64,
    #[serde(default)]
    pub playback_count: u64,
    #[serde(default)]
    pub export_count: u64,
    #[serde(default)]
    pub working_time_seconds: u64,
}

// ───────────────────────────────────────────────
// 音轨数据文件 (.lmtrack)
// ───────────────────────────────────────────────

/// .lmtrack 文件头（8 bytes，未压缩）
#[derive(Debug, Clone, Copy)]
pub struct LmtrackHeader {
    /// 魔数: b"LMTR"
    pub magic: [u8; 4],
    /// 格式版本: 1
    pub version: u16,
    /// 所属音轨编号
    pub track_id: u16,
}

impl LmtrackHeader {
    pub const SIZE: usize = 8;
    pub const MAGIC: &[u8; 4] = b"LMTR";
    pub const CURRENT_VERSION: u16 = 1;

    pub fn new(track_id: u16) -> Self {
        Self {
            magic: *Self::MAGIC,
            version: Self::CURRENT_VERSION,
            track_id,
        }
    }

    /// 编码为字节数组
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.track_id.to_le_bytes());
        buf
    }

    /// 从字节数组解码
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let track_id = u16::from_le_bytes([bytes[6], bytes[7]]);
        Some(Self {
            magic,
            version,
            track_id,
        })
    }
}

/// 音轨元数据（存储在 .lmtrack 内）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMeta {
    pub track_id: u16,
    pub name: String,
    pub channel: u8,
    pub port: u8,
    pub visibility: TrackVisibilitySer,
    pub solo: bool,
    pub is_drum: bool,
    pub max_tick: u32,
}

/// .lmtrack 文件的主体数据（序列化后压缩存储）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmtrackData {
    pub meta: TrackMeta,
    /// CompactEvent 数组的扁平字节表示（每个事件 12 字节）
    pub events: Vec<u8>,
    /// 事件总数
    pub event_count: u64,
    /// 音符总数（NoteOn 事件数）
    pub note_count: u64,
}

// ───────────────────────────────────────────────
// 辅助数据文件
// ───────────────────────────────────────────────

/// .lmtemp - 全局速度变化数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmtempData {
    pub tempo_changes: Vec<(u32, f32)>,
    pub default_bpm: f32,
}

/// .lmsig - 拍号/调号数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmsigData {
    pub time_signatures: Vec<(u32, u8, u8)>,
    pub key_signatures: Vec<(u32, i8, bool)>,
}

/// .lmctl - 控制事件数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmctlData {
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    pub pitch_bends: Vec<(u32, u16, u8, i16)>,
}

/// .lmnames - 音轨名称映射
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LmnamesData {
    pub track_names: Vec<Option<String>>,
}

// ───────────────────────────────────────────────
// 导入数据缓存 (.lmloaded)
// ───────────────────────────────────────────────

/// .lmloaded 文件头（8 bytes）
#[derive(Debug, Clone, Copy)]
pub struct LmloadedHeader {
    pub magic: [u8; 4],
    pub version: u16,
    pub format_type: LoadedFormatCode,
    pub _reserved: u8,
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum LoadedFormatCode {
    Mid = 0,
    Dms = 1,
    Lmpj = 2,
}

/// MIDI 导入缓存数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedMidiData {
    /// 原始文件信息
    pub original_info: lumino_core::MidiInfo,
    /// 原始 MIDI 字节
    pub raw_midi_bytes: Vec<u8>,
    /// 是否已预解析
    pub is_parsed: bool,
    /// 预解析的事件数据（CompactEvent 扁平数组）
    pub parsed_events: Option<Vec<u8>>,
    /// 预解析的音轨范围
    pub track_event_ranges: Option<Vec<(usize, usize)>>,
    /// 预解析的 tempo 变化
    pub parsed_tempo_changes: Option<Vec<(u32, f32)>>,
    /// 导入时间
    pub imported_at: String,
}

/// DMS 导入缓存数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedDmsData {
    pub original_info: lumino_core::DmsInfo,
    /// 解压后的 DMS 原始数据
    pub raw_dms_data: Vec<u8>,
    /// 是否已转换为 MIDI
    pub converted_to_midi: bool,
    /// 转换后的 MIDI 字节
    pub converted_midi_bytes: Option<Vec<u8>>,
    pub imported_at: String,
}

/// LMPJ 导入缓存数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedLmpjData {
    pub original_info: lumino_core::MidiInfo,
    pub midi_info: lumino_core::MidiInfo,
    pub midi_data: Vec<u8>,
    pub imported_at: String,
}

// ───────────────────────────────────────────────
// 归档格式
// ───────────────────────────────────────────────

/// .lmpj 单文件归档头部
#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    /// 魔数: b"LMPJ"
    pub magic: [u8; 4],
    /// 归档格式版本: 1
    pub version: u16,
    /// 压缩标志位: 0x01=zstd
    pub compression_flags: u8,
    /// 文件表在归档中的偏移量
    pub file_table_offset: u64,
    /// 文件表压缩后大小
    pub file_table_compressed_size: u64,
    /// 文件表原始大小
    pub file_table_original_size: u64,
    /// 创建时间戳（unix seconds）
    pub created_at: u64,
    /// 保留字段
    pub _reserved: [u8; 16],
}

impl ArchiveHeader {
    pub const SIZE: usize = 4 + 2 + 1 + 8 + 8 + 8 + 8 + 16;
    pub const MAGIC: &[u8; 4] = b"LMPJ";
    pub const CURRENT_VERSION: u16 = 1;
    pub const FLAG_ZSTD: u8 = 0x01;

    pub fn new(
        file_table_offset: u64,
        file_table_compressed_size: u64,
        file_table_original_size: u64,
    ) -> Self {
        Self {
            magic: *Self::MAGIC,
            version: Self::CURRENT_VERSION,
            compression_flags: Self::FLAG_ZSTD,
            file_table_offset,
            file_table_compressed_size,
            file_table_original_size,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            _reserved: [0; 16],
        }
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let mut off = 0usize;

        buf[off..off + 4].copy_from_slice(&self.magic);
        off += 4;
        buf[off..off + 2].copy_from_slice(&self.version.to_le_bytes());
        off += 2;
        buf[off] = self.compression_flags;
        off += 1;
        buf[off..off + 8].copy_from_slice(&self.file_table_offset.to_le_bytes());
        off += 8;
        buf[off..off + 8].copy_from_slice(&self.file_table_compressed_size.to_le_bytes());
        off += 8;
        buf[off..off + 8].copy_from_slice(&self.file_table_original_size.to_le_bytes());
        off += 8;
        buf[off..off + 8].copy_from_slice(&self.created_at.to_le_bytes());
        off += 8;
        buf[off..off + 16].copy_from_slice(&self._reserved);

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let compression_flags = bytes[6];
        let file_table_offset = u64::from_le_bytes([
            bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14],
        ]);
        let file_table_compressed_size = u64::from_le_bytes([
            bytes[15], bytes[16], bytes[17], bytes[18],
            bytes[19], bytes[20], bytes[21], bytes[22],
        ]);
        let file_table_original_size = u64::from_le_bytes([
            bytes[23], bytes[24], bytes[25], bytes[26],
            bytes[27], bytes[28], bytes[29], bytes[30],
        ]);
        let created_at = u64::from_le_bytes([
            bytes[31], bytes[32], bytes[33], bytes[34],
            bytes[35], bytes[36], bytes[37], bytes[38],
        ]);
        let mut _reserved = [0u8; 16];
        _reserved.copy_from_slice(&bytes[39..55]);

        Some(Self {
            magic,
            version,
            compression_flags,
            file_table_offset,
            file_table_compressed_size,
            file_table_original_size,
            created_at,
            _reserved,
        })
    }
}

/// 归档中的文件表条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    /// 文件在归档内的路径（如 "data/project/tracks/000.lmtrack"）
    pub path: String,
    /// 数据在归档中的偏移量
    pub data_offset: u64,
    /// 数据压缩后大小
    pub compressed_size: u64,
    /// 数据原始大小
    pub original_size: u64,
    /// CRC32 校验值
    pub crc32: u32,
    /// 是否经过压缩
    pub is_compressed: bool,
}

/// 文件表（序列化结构）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTable {
    pub entries: Vec<FileEntry>,
}

impl FileTable {
    pub fn new(entries: Vec<FileEntry>) -> Self {
        Self { entries }
    }

    /// 编码为字节（bincode + zstd）
    pub fn encode(&self) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let serialized = bincode::serialize(self)?;
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3)?;
        Ok(compressed)
    }

    /// 从字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let decompressed = zstd::stream::decode_all(std::io::Cursor::new(bytes))?;
        let table = bincode::deserialize(&decompressed)?;
        Ok(table)
    }
}

// ───────────────────────────────────────────────
// 内存中的工程表示
// ───────────────────────────────────────────────

/// 内存中的 Lumino 工程
#[derive(Debug)]
pub struct LuminoProject {
    pub metadata: ProjectMetadata,
    pub tracks: Vec<TrackSlot>,
    pub tempo_changes: Vec<(u32, f32)>,
    pub time_signatures: Vec<(u32, u8, u8)>,
    pub key_signatures: Vec<(u32, i8, bool)>,
    pub control_changes: Vec<(u32, u16, u8, u8, u8)>,
    pub program_changes: Vec<(u32, u16, u8, u8)>,
    pub loaded_files: Vec<LoadedFileEntry>,
}

/// 音轨槽 — 支持懒加载和修改追踪
#[derive(Debug)]
pub enum TrackSlot {
    /// 未加载（数据仅在磁盘上）
    Unloaded {
        track_id: u16,
        path: PathBuf,
    },
    /// 已加载
    Loaded(LmtrackData),
    /// 已修改（需要保存）
    Modified(LmtrackData),
}
