//! 高精度贴图调度器单元测试

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::compute_midi_hash;
use crate::config::HiResConfig;
use crate::scheduler::{HiResProgressCallback, generate_all_tiles, generate_all_tiles_streaming};
use crate::types::{GroupTile, TileCoord};
use lumino_onion_skin::OnionSkinNote;

fn note(start: u32, end: u32, key: u8, color: [u8; 4]) -> OnionSkinNote {
    OnionSkinNote::from_ms(start as f32, end as f32, key, color)
}

fn test_config() -> (HiResConfig, String) {
    let mut config = HiResConfig::default();
    let dir = std::env::temp_dir()
        .join("lumino-hires-sched-test")
        .join(format!("{}-{}", std::process::id(), unique_id()));
    let _ = std::fs::remove_dir_all(&dir);
    config.cache_dir = dir.clone();
    (config, compute_midi_hash(b"sched-test"))
}

fn unique_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::SeqCst)
}

fn cleanup(config: &HiResConfig) {
    let _ = std::fs::remove_dir_all(&config.cache_dir);
}

fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [
        pixels[idx],
        pixels[idx + 1],
        pixels[idx + 2],
        pixels[idx + 3],
    ]
}

#[test]
fn test_generate_empty_notes() {
    let (config, hash) = test_config();
    let result = generate_all_tiles(&mut [], &config, 1920, 128, 30720, &hash, None);
    assert!(result.is_empty());
    cleanup(&config);
}

#[test]
fn test_generate_empty_ticks() {
    let (config, hash) = test_config();
    let mut notes = vec![vec![note(0, 100, 60, [255, 0, 0, 255])]];
    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 0, &hash, None);
    assert!(result.is_empty());
    cleanup(&config);
}

#[test]
fn test_generate_single_group() {
    // 3 轨，1 个时间组（total_ticks=30720）
    let (config, hash) = test_config();
    let mut notes = vec![
        vec![note(0, 15360, 60, [255, 0, 0, 255])],
        vec![note(0, 15360, 61, [0, 255, 0, 255])],
        vec![note(15360, 30720, 60, [0, 0, 255, 255])],
    ];

    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 30720, &hash, None);

    // 1 音轨组 × 1 时间组 = 1 贴图
    assert_eq!(result.len(), 1);
    let coord = TileCoord::new(0, 0);
    let tile = result.get(&coord).expect("应有 (0,0) 贴图");
    assert_eq!(tile.track_range, (0, 3));
    assert_eq!(tile.track_count(), 3);

    // key=60 左半红，右半蓝（track2 覆盖 track0）
    assert_eq!(pixel_at(&tile.pixels, 1920, 0, 60), [255, 0, 0, 255]);
    assert_eq!(pixel_at(&tile.pixels, 1920, 1000, 60), [0, 0, 255, 255]);
    // key=61 左半绿
    assert_eq!(pixel_at(&tile.pixels, 1920, 0, 61), [0, 255, 0, 255]);

    cleanup(&config);
}

#[test]
fn test_generate_multi_track_groups() {
    // 10 轨 → 2 音轨组（8+2），1 时间组
    let (config, hash) = test_config();
    let mut notes: Vec<Vec<OnionSkinNote>> = (0..10)
        .map(|i| vec![note(0, 100, i, [i, 0, 0, 255])])
        .collect();

    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 30720, &hash, None);

    // 2 音轨组 × 1 时间组 = 2 贴图
    assert_eq!(result.len(), 2);
    let g0 = result.get(&TileCoord::new(0, 0)).expect("音轨组0");
    let g1 = result.get(&TileCoord::new(1, 0)).expect("音轨组1");
    assert_eq!(g0.track_range, (0, 8));
    assert_eq!(g1.track_range, (8, 10));
    assert_eq!(g0.track_count(), 8);
    assert_eq!(g1.track_count(), 2);

    cleanup(&config);
}

#[test]
fn test_generate_multi_time_groups() {
    // 1 轨，2 时间组（total_ticks=61440 = 2×30720）
    let (config, hash) = test_config();
    let mut notes = vec![vec![
        note(0, 15360, 60, [255, 0, 0, 255]),     // 组0
        note(40000, 50000, 64, [0, 0, 255, 255]), // 组1
    ]];

    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 61440, &hash, None);

    // 1 音轨组 × 2 时间组 = 2 贴图
    assert_eq!(result.len(), 2);
    let g0 = result.get(&TileCoord::new(0, 0)).expect("时间组0");
    let g1 = result.get(&TileCoord::new(0, 1)).expect("时间组1");

    // 组0 key=60 有红色
    assert_eq!(pixel_at(&g0.pixels, 1920, 0, 60), [255, 0, 0, 255]);
    // 组1 key=64 有蓝色（音符 40000 在组1 内，偏移 9280 tick → x≈580）
    let x_in_g1 = ((40000u32 - 30720) as f32 / 30720.0 * 1920.0) as u32;
    assert_eq!(pixel_at(&g1.pixels, 1920, x_in_g1, 64), [0, 0, 255, 255]);

    cleanup(&config);
}

#[test]
fn test_cache_hit_skips_generation() {
    // 第一次生成写缓存，第二次生成应从缓存读（像素一致）
    let (config, hash) = test_config();
    let mut notes = vec![vec![note(0, 15360, 60, [255, 0, 0, 255])]];

    let first = generate_all_tiles(&mut notes, &config, 1920, 128, 30720, &hash, None);
    let second = generate_all_tiles(&mut notes, &config, 1920, 128, 30720, &hash, None);

    let t1 = first
        .get(&TileCoord::new(0, 0))
        .expect("第一次生成应有 Tile (0,0)");
    let t2 = second
        .get(&TileCoord::new(0, 0))
        .expect("第二次生成应有 Tile (0,0)");
    assert_eq!(*t1.pixels, *t2.pixels, "缓存命中应产生相同像素");

    cleanup(&config);
}

#[test]
fn test_progress_callback_invoked() {
    let (config, hash) = test_config();
    let mut notes = vec![
        vec![note(0, 100, 60, [255, 0, 0, 255])],
        vec![note(0, 100, 61, [0, 255, 0, 255])],
    ];

    let call_count = Arc::new(AtomicUsize::new(0));
    let final_pct = Arc::new(Mutex::new(0.0f32));
    let cb_count = call_count.clone();
    let cb_pct = final_pct.clone();
    let cb: HiResProgressCallback = Arc::new(move |_msg, pct| {
        cb_count.fetch_add(1, Ordering::SeqCst);
        *cb_pct.lock().expect("Mutex 未 poison") = pct;
    });

    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 30720, &hash, Some(cb));

    assert_eq!(result.len(), 1);
    // 至少调用：1 次进度 + 1 次完成
    assert!(call_count.load(Ordering::SeqCst) >= 2);
    // 最终 pct 应为 1.0
    let pct = *final_pct.lock().expect("Mutex 未 poison");
    assert!((pct - 1.0).abs() < 0.001, "最终进度应为 1.0，实际 {pct}");

    cleanup(&config);
}

#[test]
fn test_streaming_callback_per_tile() {
    // ★ 跨 track_group 合并：2 个 time_group 各生成一张全轨合并贴图，共 2 张 ★
    let (config, hash) = test_config();
    let mut notes: Vec<Vec<OnionSkinNote>> = (0..10)
        .map(|i| {
            let key = i as u8;
            // 轨 i 在 time_group 0 和 1 各放一个音符
            vec![
                note(0, 100, key, [i as u8, 0, 0, 255]),
                note(30720, 30820, key, [i as u8, 0, 0, 255]),
            ]
        })
        .collect();

    let received = Arc::new(Mutex::new(Vec::new()));
    let received_cb = received.clone();
    let cb = move |time_group: u32, tile: GroupTile| {
        received_cb.lock().expect("Mutex 未 poison").push((
            time_group,
            tile.coord,
            tile.pixels.clone(),
        ));
    };

    generate_all_tiles_streaming(&mut notes, &config, 1920, 128, 61440, &hash, None, &cb);

    let guard = received.lock().expect("Mutex 未 poison");
    assert_eq!(
        guard.len(),
        2,
        "应收到 2 张全轨合并流式贴图（每 time_group 一张）"
    );

    // 坐标：跨 track_group 合并后只用 (0, 0) 和 (0, 1)
    let coords: std::collections::HashSet<_> = guard.iter().map(|(_, c, _)| *c).collect();
    assert!(coords.contains(&TileCoord::new(0, 0)));
    assert!(coords.contains(&TileCoord::new(0, 1)));
    assert_eq!(coords.len(), 2);

    // 每张合并贴图应包含全部 10 轨的音符（跨 track_group 合并验证）
    for (time_group, coord, pixels) in guard.iter() {
        assert_eq!(coord.track_group, 0, "合并后 track_group 固定为 0");
        for t in 0..10 {
            let key = t as u8;
            let x = 0u32;
            assert_eq!(
                pixel_at(pixels, 1920, x, key as u32),
                [t as u8, 0, 0, 255],
                "time_group={}, track={} 应有颜色（跨 track_group 合并）",
                time_group,
                t
            );
        }
    }

    cleanup(&config);
}
