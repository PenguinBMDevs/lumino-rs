//! `.lmocache` 硬盘缓存读写与失效校验
//!
//! 单音轨贴图块落盘格式：
//! ```text
//! 偏移  长度  字段
//! 0     8     magic: b"LMOCache"
//! 8     2     version: u16 (小端，当前 = 1)
//! 10    4     meta_len: u32 (小端，元数据 bincode 字节数)
//! 14    N     metadata (bincode): CacheMeta
//! 14+N  *     zstd level 3 压缩的 RGBA8 像素
//! ```
//!
//! 文件命名：`{midi_hash}_t{track_idx}_g{time_group}.lmocache`
//! 按 MIDI 内容哈希分桶，不同 MIDI 不会串台。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::types::TrackTile;

/// 缓存文件 magic 标识
const MAGIC: &[u8; 8] = b"LMOCache";

/// 缓存格式版本
const VERSION: u16 = 1;

/// zstd 压缩级别（与 LMPJ 工程文件一致，快速压缩）
const ZSTD_LEVEL: i32 = 3;

/// 缓存元数据（随像素一起落盘，用于失效校验）
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CacheMeta {
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

impl CacheMeta {
    /// 从贴图块与当前规格构造元数据
    pub fn from_tile(tile: &TrackTile, key_count: u16, ppq: u16, measures_per_group: u32) -> Self {
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
pub enum CacheError {
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
pub fn compute_midi_hash(data: &[u8]) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(data))
}

/// 生成缓存文件名
fn cache_file_name(midi_hash: &str, track_idx: u16, time_group: u32) -> String {
    format!("{midi_hash}_t{track_idx}_g{time_group}.lmocache")
}

/// 生成缓存文件完整路径
pub fn cache_path(cache_dir: &Path, midi_hash: &str, track_idx: u16, time_group: u32) -> PathBuf {
    cache_dir.join(cache_file_name(midi_hash, track_idx, time_group))
}

/// 写入单音轨贴图缓存
///
/// 成功返回写入的文件路径。若缓存目录不存在会自动创建。
pub fn write_track_tile_cache(
    cache_dir: &Path,
    midi_hash: &str,
    tile: &TrackTile,
    meta: &CacheMeta,
) -> Result<PathBuf, CacheError> {
    std::fs::create_dir_all(cache_dir)?;
    let path = cache_path(cache_dir, midi_hash, tile.track_idx, tile.time_group);
    write_cache_file(&path, tile, meta)?;
    Ok(path)
}

fn write_cache_file(path: &Path, tile: &TrackTile, meta: &CacheMeta) -> Result<(), CacheError> {
    let meta_bytes = bincode::serialize(meta).map_err(|e| CacheError::MetaCodec(e.to_string()))?;
    let compressed = zstd::stream::encode_all(tile.pixels.as_slice(), ZSTD_LEVEL)
        .map_err(|e| CacheError::PixelCodec(e.to_string()))?;

    let mut file = std::fs::File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    file.write_all(&(meta_bytes.len() as u32).to_le_bytes())?;
    file.write_all(&meta_bytes)?;
    file.write_all(&compressed)?;
    Ok(())
}

/// 读取单音轨贴图缓存（含失效校验）
///
/// 文件不存在返回 `Ok(None)`。文件存在但 magic/version/规格不匹配返回 `Err`，
/// 调用方应捕获后删除损坏文件并重生成。
pub fn read_track_tile_cache(
    cache_dir: &Path,
    midi_hash: &str,
    track_idx: u16,
    time_group: u32,
    expected: &CacheMeta,
) -> Result<Option<TrackTile>, CacheError> {
    let path = cache_path(cache_dir, midi_hash, track_idx, time_group);
    if !path.exists() {
        return Ok(None);
    }
    read_cache_file(&path, track_idx, time_group, expected).map(Some)
}

fn read_cache_file(
    path: &Path,
    track_idx: u16,
    time_group: u32,
    expected: &CacheMeta,
) -> Result<TrackTile, CacheError> {
    let mut file = std::fs::File::open(path)?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(CacheError::MagicMismatch {
            expected: *MAGIC,
            actual: magic,
        });
    }

    let mut version_buf = [0u8; 2];
    file.read_exact(&mut version_buf)?;
    let version = u16::from_le_bytes(version_buf);
    if version != VERSION {
        return Err(CacheError::VersionMismatch {
            expected: VERSION,
            actual: version,
        });
    }

    let mut meta_len_buf = [0u8; 4];
    file.read_exact(&mut meta_len_buf)?;
    let meta_len = u32::from_le_bytes(meta_len_buf) as usize;

    let mut meta_bytes = vec![0u8; meta_len];
    file.read_exact(&mut meta_bytes)?;
    let meta: CacheMeta =
        bincode::deserialize(&meta_bytes).map_err(|e| CacheError::MetaCodec(e.to_string()))?;

    if !meta.matches_spec(
        expected.width,
        expected.height,
        expected.key_count,
        expected.ppq,
        expected.measures_per_group,
    ) {
        return Err(CacheError::SpecMismatch(format!(
            "缓存元数据 {meta:?} 与期望规格 (w={},h={},key={},ppq={},mpg={}) 不符",
            expected.width,
            expected.height,
            expected.key_count,
            expected.ppq,
            expected.measures_per_group
        )));
    }

    let mut compressed = Vec::new();
    file.read_to_end(&mut compressed)?;
    let pixels = zstd::stream::decode_all(compressed.as_slice())
        .map_err(|e| CacheError::PixelCodec(e.to_string()))?;

    Ok(TrackTile {
        track_idx,
        time_group,
        pixels,
        width: meta.width,
        height: meta.height,
        tick_start: meta.tick_start,
        tick_end: meta.tick_end,
    })
}

/// 清理指定 MIDI 的所有缓存文件，返回删除数量
pub fn clear_midi_cache(cache_dir: &Path, midi_hash: &str) -> Result<u32, CacheError> {
    let prefix = format!("{midi_hash}_");
    let mut count = 0;
    if cache_dir.exists() {
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && name.starts_with(&prefix)
                && name.ends_with(".lmocache")
            {
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// 清理缓存目录下全部 `.lmocache` 文件，返回删除数量
pub fn clear_all_cache(cache_dir: &Path) -> Result<u32, CacheError> {
    let mut count = 0;
    if cache_dir.exists() {
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".lmocache")
            {
                std::fs::remove_file(entry.path())?;
                count += 1;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造测试用临时缓存目录（按进程 id + 计数隔离）
    fn test_cache_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join("lumino-onion-hires-test")
            .join(format!("{label}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_tile(track_idx: u16, time_group: u32) -> TrackTile {
        let width = 1920u32;
        let height = 128u32;
        // 用 track_idx 作像素值便于校验往返
        let pixels = vec![track_idx as u8; (width * height * 4) as usize];
        TrackTile {
            track_idx,
            time_group,
            pixels,
            width,
            height,
            tick_start: time_group * 30720,
            tick_end: (time_group + 1) * 30720,
        }
    }

    fn sample_meta(tile: &TrackTile) -> CacheMeta {
        CacheMeta::from_tile(tile, 128, 1920, 4)
    }

    #[test]
    fn test_compute_midi_hash_stable() {
        let data = b"hello midi";
        let h1 = compute_midi_hash(data);
        let h2 = compute_midi_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
        // 不同数据不同哈希
        let h3 = compute_midi_hash(b"hello midi!");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_cache_roundtrip() {
        let dir = test_cache_dir("roundtrip");
        let tile = sample_tile(3, 5);
        let meta = sample_meta(&tile);
        let hash = compute_midi_hash(b"test-midi");

        let written = write_track_tile_cache(&dir, &hash, &tile, &meta).unwrap();
        assert!(written.exists());

        let read = read_track_tile_cache(&dir, &hash, 3, 5, &meta).unwrap();
        let read = read.expect("应读到缓存");
        assert_eq!(read.track_idx, 3);
        assert_eq!(read.time_group, 5);
        assert_eq!(read.width, 1920);
        assert_eq!(read.height, 128);
        assert_eq!(read.pixels, tile.pixels);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let dir = test_cache_dir("miss");
        let meta = sample_meta(&sample_tile(0, 0));
        let hash = compute_midi_hash(b"no-such-midi");
        let read = read_track_tile_cache(&dir, &hash, 0, 0, &meta).unwrap();
        assert!(read.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_spec_mismatch() {
        let dir = test_cache_dir("spec");
        let tile = sample_tile(1, 0);
        let meta = sample_meta(&tile); // key_count=128, ppq=1920, mpg=4
        let hash = compute_midi_hash(b"spec-test");

        write_track_tile_cache(&dir, &hash, &tile, &meta).unwrap();

        // 用不同规格读取（ppq 变了）→ SpecMismatch
        let wrong_meta = CacheMeta { ppq: 480, ..meta };
        let result = read_track_tile_cache(&dir, &hash, 1, 0, &wrong_meta);
        assert!(matches!(result, Err(CacheError::SpecMismatch(_))));

        // 用不同 measures_per_group 读取 → SpecMismatch
        let wrong_meta2 = CacheMeta {
            measures_per_group: 8,
            ..meta
        };
        let result2 = read_track_tile_cache(&dir, &hash, 1, 0, &wrong_meta2);
        assert!(matches!(result2, Err(CacheError::SpecMismatch(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_magic_corrupt() {
        let dir = test_cache_dir("magic");
        let tile = sample_tile(0, 0);
        let meta = sample_meta(&tile);
        let hash = compute_midi_hash(b"corrupt");

        let path = write_track_tile_cache(&dir, &hash, &tile, &meta).unwrap();
        // 破坏 magic
        std::fs::write(&path, b"XXXXXXX").unwrap();
        let result = read_track_tile_cache(&dir, &hash, 0, 0, &meta);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_midi_cache() {
        let dir = test_cache_dir("clear");
        let hash = compute_midi_hash(b"clear-test");
        // 写 3 个不同 tile
        for (t, g) in [(0u16, 0u32), (1, 0), (0, 1)] {
            let tile = sample_tile(t, g);
            let meta = sample_meta(&tile);
            write_track_tile_cache(&dir, &hash, &tile, &meta).unwrap();
        }
        // 写一个别的 MIDI 的缓存
        let other_hash = compute_midi_hash(b"other");
        let other_tile = sample_tile(0, 0);
        let other_meta = sample_meta(&other_tile);
        write_track_tile_cache(&dir, &other_hash, &other_tile, &other_meta).unwrap();

        // 只清当前 MIDI
        let removed = clear_midi_cache(&dir, &hash).unwrap();
        assert_eq!(removed, 3);

        // 另一个 MIDI 的缓存还在
        let read = read_track_tile_cache(&dir, &other_hash, 0, 0, &other_meta).unwrap();
        assert!(read.is_some());

        // 清全部
        let removed_all = clear_all_cache(&dir).unwrap();
        assert_eq!(removed_all, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_nonexistent_dir() {
        let dir = test_cache_dir("nonexistent");
        // 目录不存在，清理应返回 0 不报错
        let removed = clear_all_cache(&dir).unwrap();
        assert_eq!(removed, 0);
        let removed = clear_midi_cache(&dir, "deadbeef").unwrap();
        assert_eq!(removed, 0);
    }
}
