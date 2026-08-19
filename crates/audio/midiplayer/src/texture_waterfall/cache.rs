//! `.lmocache` 硬盘缓存读写与失效校验
//!
//! 单音轨贴图块落盘格式：
//! ```text
//! 偏移  长度  字段
//! 0     8     magic: b"LMOCache"
//! 8     2     version: u16 (小端，当前 = 1)
//! 10    4     meta_len: u32 (小端，元数据 bincode 字节数)
//! 14    N     metadata (bincode): WaterfallCacheMeta
//! 14+N  *     zstd level 3 压缩的 RGBA8 像素
//! ```
//!
//! 文件命名：`{midi_hash}_t{track_idx}_g{time_group}.lmocache`
//! 按 MIDI 内容哈希分桶，不同 MIDI 不会串台。
//!
//! # 内部模块
//!
//! - [`core`]：缓存核心结构体、错误类型与工具函数
//! - [`io`]：磁盘读写操作（save/load）
//! - [`cleanup`]：清理与淘汰逻辑

mod cleanup;
mod core;
mod io;

pub use cleanup::{clear_all_waterfall_cache, clear_midi_waterfall_cache};
pub use core::{
    WaterfallCacheError, WaterfallCacheMeta, compute_waterfall_cache_hash, waterfall_cache_path,
};
pub use io::{read_waterfall_track_tile_cache, write_waterfall_track_tile_cache};

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::texture_waterfall::types::WaterfallTrackTile;

    use super::*;

    /// 构造测试用临时缓存目录（按进程 id + 计数隔离）
    fn test_cache_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir()
            .join("lumino-onion-TextureWaterfall-test")
            .join(format!("{label}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_tile(track_idx: u16, time_group: u32) -> WaterfallTrackTile {
        let width = 1920u32;
        let height = 128u32;
        // 用 track_idx 作像素值便于校验往返
        let pixels = vec![track_idx as u8; (width * height * 4) as usize];
        let tick_start = time_group * 30720;
        let tick_end = (time_group + 1) * 30720;
        WaterfallTrackTile::new(
            track_idx, time_group, pixels, width, height, tick_start, tick_end,
        )
    }

    fn sample_meta(tile: &WaterfallTrackTile) -> WaterfallCacheMeta {
        WaterfallCacheMeta::from_tile(tile, 128, 1920, 4)
    }

    #[test]
    fn test_compute_midi_hash_stable() {
        let data = b"hello midi";
        let h1 = compute_waterfall_cache_hash(data);
        let h2 = compute_waterfall_cache_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
        // 不同数据不同哈希
        let h3 = compute_waterfall_cache_hash(b"hello midi!");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_cache_roundtrip() {
        let dir = test_cache_dir("roundtrip");
        let tile = sample_tile(3, 5);
        let meta = sample_meta(&tile);
        let hash = compute_waterfall_cache_hash(b"test-midi");

        let written =
            write_waterfall_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");
        assert!(written.exists());

        let read = read_waterfall_track_tile_cache(&dir, &hash, 3, 5, &meta).expect("读缓存应成功");
        let read = read.expect("应读到缓存");
        assert_eq!(read.track_idx, 3);
        assert_eq!(read.time_group, 5);
        assert_eq!(read.width, 1920);
        assert_eq!(read.height, 128);
        assert_eq!(*read.pixels, *tile.pixels);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let dir = test_cache_dir("miss");
        let meta = sample_meta(&sample_tile(0, 0));
        let hash = compute_waterfall_cache_hash(b"no-such-midi");
        let read = read_waterfall_track_tile_cache(&dir, &hash, 0, 0, &meta)
            .expect("缓存 miss 时读缓存应返回 Ok(None)");
        assert!(read.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_spec_mismatch() {
        let dir = test_cache_dir("spec");
        let tile = sample_tile(1, 0);
        let meta = sample_meta(&tile); // key_count=128, ppq=1920, mpg=4
        let hash = compute_waterfall_cache_hash(b"spec-test");

        write_waterfall_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");

        // 用不同规格读取（ppq 变了）→ SpecMismatch
        let wrong_meta = WaterfallCacheMeta { ppq: 480, ..meta };
        let result = read_waterfall_track_tile_cache(&dir, &hash, 1, 0, &wrong_meta);
        assert!(matches!(result, Err(WaterfallCacheError::SpecMismatch(_))));

        // 用不同 measures_per_group 读取 → SpecMismatch
        let wrong_meta2 = WaterfallCacheMeta {
            measures_per_group: 8,
            ..meta
        };
        let result2 = read_waterfall_track_tile_cache(&dir, &hash, 1, 0, &wrong_meta2);
        assert!(matches!(result2, Err(WaterfallCacheError::SpecMismatch(_))));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_cache_magic_corrupt() {
        let dir = test_cache_dir("magic");
        let tile = sample_tile(0, 0);
        let meta = sample_meta(&tile);
        let hash = compute_waterfall_cache_hash(b"corrupt");

        let path =
            write_waterfall_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");
        // 破坏 magic
        std::fs::write(&path, b"XXXXXXX").expect("写入损坏数据应成功");
        let result = read_waterfall_track_tile_cache(&dir, &hash, 0, 0, &meta);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_midi_cache() {
        let dir = test_cache_dir("clear");
        let hash = compute_waterfall_cache_hash(b"clear-test");
        // 写 3 个不同 tile
        for (t, g) in [(0u16, 0u32), (1, 0), (0, 1)] {
            let tile = sample_tile(t, g);
            let meta = sample_meta(&tile);
            write_waterfall_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");
        }
        // 写一个别的 MIDI 的缓存
        let other_hash = compute_waterfall_cache_hash(b"other");
        let other_tile = sample_tile(0, 0);
        let other_meta = sample_meta(&other_tile);
        write_waterfall_track_tile_cache(&dir, &other_hash, &other_tile, &other_meta)
            .expect("写 other MIDI 缓存应成功");

        // 只清当前 MIDI
        let removed = clear_midi_waterfall_cache(&dir, &hash).expect("清理 MIDI 缓存应成功");
        assert_eq!(removed, 3);

        // 另一个 MIDI 的缓存还在
        let read = read_waterfall_track_tile_cache(&dir, &other_hash, 0, 0, &other_meta)
            .expect("读 other MIDI 缓存应成功");
        assert!(read.is_some());

        // 清全部
        let removed_all = clear_all_waterfall_cache(&dir).expect("清理全部缓存应成功");
        assert_eq!(removed_all, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_clear_nonexistent_dir() {
        let dir = test_cache_dir("nonexistent");
        // 目录不存在，清理应返回 0 不报错
        let removed = clear_all_waterfall_cache(&dir).expect("清理不存在的目录应返回 Ok(0)");
        assert_eq!(removed, 0);
        let removed = clear_midi_waterfall_cache(&dir, "deadbeef")
            .expect("清理不存在的 MIDI 缓存应返回 Ok(0)");
        assert_eq!(removed, 0);
    }
}
