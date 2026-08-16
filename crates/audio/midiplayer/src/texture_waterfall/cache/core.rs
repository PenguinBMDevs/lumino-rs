//! 缓存核心结构体、错误类型与工具函数
//!
//! 包含 `WaterfallCacheMeta`、`WaterfallCacheError`、哈希计算和路径生成。
//! 不涉及磁盘 IO（见 `super::io`）和清理逻辑（见 `super::cleanup`）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::texture_waterfall::types::WaterfallTrackTile;

/// 缓存文件 magic 标识
pub(super) const MAGIC: &[u8; 8] = b"LMOCache";

/// 缓存格式版本
pub(super) const VERSION: u16 = 1;

/// zstd 压缩级别（与 LMPJ 工程文件一致，快速压缩）
pub(super) const ZSTD_LEVEL: i32 = 3;

/// 缓存元数据（随像素一起落盘，用于失效校验）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WaterfallCacheMeta {
    pub track_idx: u16,
    pub time_group: u32,
    pub width: u32,
    pub height: u32,
    pub tick_start: u32,
    pub tick_end: u32,
    pub key_count: u16,
    pub ppq: u16,
    pub measures_per_group: u32,
}

impl WaterfallCacheMeta {
    /// 从贴图块与当前规格构造元数据
    pub fn from_tile(
        tile: &WaterfallTrackTile,
        key_count: u16,
        ppq: u16,
        measures_per_group: u32,
    ) -> Self {
        Self {
            track_idx: tile.track_idx,
            time_group: tile.time_group,
            width: tile.width,
            height: tile.height,
            tick_start: tile.tick_start,
            tick_end: tile.tick_end,
            key_count,
            ppq,
            measures_per_group,
        }
    }

    /// 校验规格是否与期望一致（ppq/小节数/宽高/key数变化则缓存失效）
    pub fn matches_spec(
        &self,
        width: u32,
        height: u32,
        key_count: u16,
        ppq: u16,
        measures_per_group: u32,
    ) -> bool {
        self.width == width
            && self.height == height
            && self.key_count == key_count
            && self.ppq == ppq
            && self.measures_per_group == measures_per_group
    }
}

/// 缓存读写错误
#[derive(Debug, Error)]
pub enum WaterfallCacheError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("magic 不匹配: 期望 {expected:?}, 实际 {actual:?}")]
    MagicMismatch { expected: [u8; 8], actual: [u8; 8] },
    #[error("版本不匹配: 期望 {expected}, 实际 {actual}")]
    VersionMismatch { expected: u16, actual: u16 },
    #[error("元数据序列化/反序列化失败: {0}")]
    MetaCodec(String),
    #[error("像素压缩/解压失败: {0}")]
    PixelCodec(String),
    #[error("规格不匹配（缓存失效）: {0}")]
    SpecMismatch(String),
}

/// 生成 MIDI 内容哈希（轻量方案：xxh3，16 位十六进制）
///
/// 非加密哈希，碰撞概率极低且 `.lmocache` 仅是缓存可容忍偶发碰撞。
/// 使用 xxh3 默认种子（0），保证跨进程、跨会话哈希稳定，使磁盘缓存真正生效。
pub fn compute_waterfall_cache_hash(data: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(data))
}

/// 生成缓存文件名
fn cache_file_name(midi_hash: &str, track_idx: u16, time_group: u32) -> String {
    format!("{midi_hash}_t{track_idx}_g{time_group}.lmocache")
}

/// 生成缓存文件完整路径
pub fn waterfall_cache_path(
    cache_dir: &Path,
    midi_hash: &str,
    track_idx: u16,
    time_group: u32,
) -> PathBuf {
    cache_dir.join(cache_file_name(midi_hash, track_idx, time_group))
}
