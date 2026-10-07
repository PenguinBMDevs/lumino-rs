//! 缓存文件的磁盘读写操作
//!
//! 提供 `write_waterfall_track_tile_cache` / `read_waterfall_track_tile_cache` 及其内部辅助函数。
//! 文件格式定义见父模块文档。

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::texture_waterfall::types::WaterfallTrackTile;

use super::core::{MAGIC, VERSION, WaterfallCacheError, WaterfallCacheMeta, ZSTD_LEVEL};

/// 元数据硬上限（64 KiB）：`meta_len` 来自文件头（不可信），防止巨量分配（DEBT-02 #119）。
const MAX_META_BYTES: usize = 64 * 1024;
/// 压缩体硬上限（256 MiB）：贴图缓存不该超过此体量，损坏文件不得触发无界读。
const MAX_COMPRESSED_BYTES: u64 = 256 << 20;
/// 解压像素硬上限（1 GiB）：防御 zstd 炸弹。
const MAX_TILE_BYTES: u64 = 1 << 30;

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
    // DEBT-02 #119：meta_len 不可信，超限直接拒绝（旧实现 vec![0; meta_len] 无界）
    if meta_len > MAX_META_BYTES {
        return Err(WaterfallCacheError::MetaCodec(format!(
            "meta 长度 {meta_len} 超过上限 {MAX_META_BYTES}，缓存已损坏"
        )));
    }

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

    // DEBT-02 #119：压缩体读取与解压输出都设上限，损坏缓存不得无界分配
    let mut compressed = Vec::new();
    (&mut file)
        .take(MAX_COMPRESSED_BYTES + 1)
        .read_to_end(&mut compressed)?;
    if compressed.len() as u64 > MAX_COMPRESSED_BYTES {
        return Err(WaterfallCacheError::PixelCodec(format!(
            "缓存压缩体超过上限 {MAX_COMPRESSED_BYTES} 字节"
        )));
    }
    let pixels = decode_zstd_limited(&compressed)?;

    let tile = WaterfallTrackTile::new(
        track_idx,
        time_group,
        pixels,
        meta.width,
        meta.height,
        meta.tick_start,
        meta.tick_end,
    );
    // DEBT-02 #119：上传前必须校验像素长度与规格一致（旧实现从不调用 validate）
    if !tile.validate() {
        return Err(WaterfallCacheError::PixelCodec(format!(
            "像素长度 {} 与规格 {}x{}x4={} 不符，缓存已损坏",
            tile.byte_len(),
            meta.width,
            meta.height,
            tile.expected_byte_len()
        )));
    }
    Ok(tile)
}

/// zstd 解压像素并限制输出上限（DEBT-02 #119）。
fn decode_zstd_limited(compressed: &[u8]) -> Result<Vec<u8>, WaterfallCacheError> {
    let mut decoder = zstd::stream::Decoder::new(std::io::Cursor::new(compressed))
        .map_err(|e| WaterfallCacheError::PixelCodec(e.to_string()))?;
    let mut pixels = Vec::new();
    (&mut decoder)
        .take(MAX_TILE_BYTES + 1)
        .read_to_end(&mut pixels)
        .map_err(|e| WaterfallCacheError::PixelCodec(e.to_string()))?;
    if pixels.len() as u64 > MAX_TILE_BYTES {
        return Err(WaterfallCacheError::PixelCodec(format!(
            "解压像素超过上限 {MAX_TILE_BYTES} 字节"
        )));
    }
    Ok(pixels)
}

// 本模块没有独立的测试——所有 IO 测试在 `super` 模块的 `tests` 中
// 通过 pub 函数覆盖全路径。
