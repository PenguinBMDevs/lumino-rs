//! 缓存文件的磁盘读写操作
//!
//! 提供 `write_waterfall_track_tile_cache` / `read_waterfall_track_tile_cache` 及其内部辅助函数。
//! 文件格式定义见父模块文档。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::texture_waterfall::types::WaterfallTrackTile;

use super::core::{MAGIC, VERSION, WaterfallCacheError, WaterfallCacheMeta, ZSTD_LEVEL};

/// 写入单音轨贴图缓存
///
/// 成功返回写入的文件路径。若缓存目录不存在会自动创建。
pub fn write_waterfall_track_tile_cache(
    cache_dir: &Path,
    midi_hash: &str,
    tile: &WaterfallTrackTile,
    meta: &WaterfallCacheMeta,
) -> Result<PathBuf, WaterfallCacheError> {
    std::fs::create_dir_all(cache_dir)?;
    let path =
        super::core::waterfall_cache_path(cache_dir, midi_hash, tile.track_idx, tile.time_group);
    write_cache_file(&path, tile, meta)?;
    Ok(path)
}

fn write_cache_file(
    path: &Path,
    tile: &WaterfallTrackTile,
    meta: &WaterfallCacheMeta,
) -> Result<(), WaterfallCacheError> {
    let meta_bytes =
        bincode::serialize(meta).map_err(|e| WaterfallCacheError::MetaCodec(e.to_string()))?;
    let compressed = zstd::stream::encode_all(tile.pixels.as_slice(), ZSTD_LEVEL)
        .map_err(|e| WaterfallCacheError::PixelCodec(e.to_string()))?;

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
pub fn read_waterfall_track_tile_cache(
    cache_dir: &Path,
    midi_hash: &str,
    track_idx: u16,
    time_group: u32,
    expected: &WaterfallCacheMeta,
) -> Result<Option<WaterfallTrackTile>, WaterfallCacheError> {
    let path = super::core::waterfall_cache_path(cache_dir, midi_hash, track_idx, time_group);
    if !path.exists() {
        return Ok(None);
    }
    read_cache_file(&path, track_idx, time_group, expected).map(Some)
}

fn read_cache_file(
    path: &Path,
    track_idx: u16,
    time_group: u32,
    expected: &WaterfallCacheMeta,
) -> Result<WaterfallTrackTile, WaterfallCacheError> {
    let mut file = std::fs::File::open(path)?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(WaterfallCacheError::MagicMismatch {
            expected: *MAGIC,
            actual: magic,
        });
    }

    let mut version_buf = [0u8; 2];
    file.read_exact(&mut version_buf)?;
    let version = u16::from_le_bytes(version_buf);
    if version != VERSION {
        return Err(WaterfallCacheError::VersionMismatch {
            expected: VERSION,
            actual: version,
        });
    }

    let mut meta_len_buf = [0u8; 4];
    file.read_exact(&mut meta_len_buf)?;
    let meta_len = u32::from_le_bytes(meta_len_buf) as usize;

    let mut meta_bytes = vec![0u8; meta_len];
    file.read_exact(&mut meta_bytes)?;
    let meta: WaterfallCacheMeta = bincode::deserialize(&meta_bytes)
        .map_err(|e| WaterfallCacheError::MetaCodec(e.to_string()))?;

    if !meta.matches_spec(
        expected.width,
        expected.height,
        expected.key_count,
        expected.ppq,
        expected.measures_per_group,
    ) {
        return Err(WaterfallCacheError::SpecMismatch(format!(
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
        .map_err(|e| WaterfallCacheError::PixelCodec(e.to_string()))?;

    Ok(WaterfallTrackTile::new(
        track_idx,
        time_group,
        pixels,
        meta.width,
        meta.height,
        meta.tick_start,
        meta.tick_end,
    ))
}

// 本模块没有独立的测试——所有 IO 测试在 `super` 模块的 `tests` 中
// 通过 pub 函数覆盖全路径。
