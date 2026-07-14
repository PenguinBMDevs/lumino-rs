//! 高精度贴图系统端到端集成测试
//!
//! 验证 generate → cache → merge 完整流程，跨模块串联。

use lumino_onion_skin::OnionSkinNote;
use lumino_onion_skin_hires::{
    CacheMeta, HiResConfig, TileCoord, compute_midi_hash, generate_track_tile, merge_group_tiles,
    read_track_tile_cache, write_track_tile_cache,
};
use std::path::PathBuf;

const WIDTH: u32 = 1920;
const KEYS: u16 = 128;
const PPQ: u16 = 1920;
/// 4 小节 × ppq × 4 = 30720 tick
const TICKS_PER_GROUP: u32 = 30720;

fn note(start: u32, end: u32, key: u8, color: [u8; 4]) -> OnionSkinNote {
    OnionSkinNote::from_ms(start as f32, end as f32, key, color)
}

fn test_cache_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir()
        .join("lumino-onion-hires-integ")
        .join(format!("{}-{id}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn test_generate_cache_roundtrip() {
    let dir = test_cache_dir();
    let cfg = HiResConfig::default();
    let hash = compute_midi_hash(b"integration-test-midi");
    let notes = vec![
        note(0, 15360, 60, [255, 0, 0, 255]),
        note(15360, 30720, 64, [0, 255, 0, 255]),
    ];

    // 生成单音轨贴图
    let tile = generate_track_tile(&notes, 0, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);
    let meta = CacheMeta::from_tile(&tile, KEYS, PPQ, cfg.measures_per_group);

    // 写缓存 → 读缓存 → 像素一致
    write_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");
    let read = read_track_tile_cache(&dir, &hash, 0, 0, &meta)
        .expect("读缓存应返回 Ok")
        .expect("缓存应存在");
    assert_eq!(*read.pixels, *tile.pixels);
    assert_eq!(read.width, WIDTH);
    assert_eq!(read.height, KEYS as u32);
    assert_eq!(read.track_idx, 0);
    assert_eq!(read.time_group, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_generate_merge_full_pipeline() {
    // 3 轨：track0 红 key60 全宽，track1 绿 key61 左半，
    // track2 蓝 key60 右半（与 track0 重叠区，后轨覆盖）
    let track0 = vec![note(0, 30720, 60, [255, 0, 0, 255])];
    let track1 = vec![note(0, 15360, 61, [0, 255, 0, 255])];
    let track2 = vec![note(15360, 30720, 60, [0, 0, 255, 255])];

    let t0 = generate_track_tile(&track0, 0, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);
    let t1 = generate_track_tile(&track1, 1, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);
    let t2 = generate_track_tile(&track2, 2, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);

    let group = merge_group_tiles(
        &[t0, t1, t2],
        TileCoord::new(0, 0),
        0,
        TICKS_PER_GROUP,
        WIDTH,
        KEYS,
        (0, 3),
    );

    // key=60 左半边（x=0）→ track0 红
    let idx_left = ((60 * WIDTH) * 4) as usize;
    assert_eq!(group.pixels[idx_left], 255);
    assert_eq!(group.pixels[idx_left + 2], 0);

    // key=60 右半边（x=1000）→ track2 蓝（覆盖 track0）
    let idx_right = ((60 * WIDTH + 1000) * 4) as usize;
    assert_eq!(group.pixels[idx_right], 0);
    assert_eq!(group.pixels[idx_right + 2], 255);

    // key=61 左半边 → track1 绿（非重叠区保留）
    let idx61 = ((61 * WIDTH) * 4) as usize;
    assert_eq!(group.pixels[idx61 + 1], 255);

    // key=61 右半边 → 透明（track1 只覆盖左半）
    let idx61_right = ((61 * WIDTH + 1000) * 4) as usize;
    assert_eq!(group.pixels[idx61_right + 3], 0);

    assert_eq!(group.track_count(), 3);
}

#[test]
fn test_cache_invalidation_on_spec_change() {
    let dir = test_cache_dir();
    let hash = compute_midi_hash(b"spec-change-test");
    let notes = vec![note(0, 100, 60, [255, 0, 0, 255])];

    let tile = generate_track_tile(&notes, 0, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);
    let meta = CacheMeta::from_tile(&tile, KEYS, PPQ, 4);
    write_track_tile_cache(&dir, &hash, &tile, &meta).expect("写缓存应成功");

    // 用不同 ppq 读 → 规格失效
    let wrong_meta = CacheMeta { ppq: 480, ..meta };
    let result = read_track_tile_cache(&dir, &hash, 0, 0, &wrong_meta);
    assert!(result.is_err());

    // 原规格仍可读
    let read = read_track_tile_cache(&dir, &hash, 0, 0, &meta).expect("读原规格缓存应成功");
    assert!(read.is_some());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_multi_time_group_layout() {
    // 验证多个时间组的贴图矩阵：音轨 0 在组 0 和组 1 各有音符
    let cfg = HiResConfig::default();
    let hash = compute_midi_hash(b"multi-group");
    let dir = test_cache_dir();

    // 组0 [0, 30720)，组1 [30720, 61440)
    let notes_group0 = vec![note(0, 15360, 60, [255, 0, 0, 255])];
    let notes_group1 = vec![note(40000, 50000, 64, [0, 0, 255, 255])];

    let tile0 = generate_track_tile(&notes_group0, 0, 0, 0, TICKS_PER_GROUP, WIDTH, KEYS);
    let tile1 = generate_track_tile(
        &notes_group1,
        0,
        1,
        TICKS_PER_GROUP,
        TICKS_PER_GROUP * 2,
        WIDTH,
        KEYS,
    );

    // 分别写缓存
    let meta0 = CacheMeta::from_tile(&tile0, KEYS, PPQ, cfg.measures_per_group);
    let meta1 = CacheMeta::from_tile(&tile1, KEYS, PPQ, cfg.measures_per_group);
    write_track_tile_cache(&dir, &hash, &tile0, &meta0).expect("写 tile0 缓存应成功");
    write_track_tile_cache(&dir, &hash, &tile1, &meta1).expect("写 tile1 缓存应成功");

    // 分别读缓存
    let read0 = read_track_tile_cache(&dir, &hash, 0, 0, &meta0)
        .expect("读 tile0 缓存应返回 Ok")
        .expect("tile0 缓存应存在");
    let read1 = read_track_tile_cache(&dir, &hash, 0, 1, &meta1)
        .expect("读 tile1 缓存应返回 Ok")
        .expect("tile1 缓存应存在");

    assert_eq!(read0.time_group, 0);
    assert_eq!(read1.time_group, 1);
    assert_eq!(*read0.pixels, *tile0.pixels);
    assert_eq!(*read1.pixels, *tile1.pixels);

    let _ = std::fs::remove_dir_all(&dir);
}
