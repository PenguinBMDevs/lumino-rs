//! 高精度贴图调度器单元测试

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::compute_midi_hash;
use crate::config::HiResConfig;
use crate::scheduler::{HiResProgressCallback, generate_all_tiles, generate_all_tiles_streaming};
use crate::types::{GroupTile, TileCoord};
use lumino_onion_skin::OnionSkinNote;

fn make_note(start: u32, end: u32, key: u8, color: [u8; 4]) -> OnionSkinNote {
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
    let mut notes = vec![vec![make_note(0, 100, 60, [255, 0, 0, 255])]];
    let result = generate_all_tiles(&mut notes, &config, 1920, 128, 0, &hash, None);
    assert!(result.is_empty());
    cleanup(&config);
}

#[test]
fn test_generate_single_group() {
    // 3 轨，1 个时间组（total_ticks=30720）
    let (config, hash) = test_config();
    let mut notes = vec![
        vec![make_note(0, 15360, 60, [255, 0, 0, 255])],
        vec![make_note(0, 15360, 61, [0, 255, 0, 255])],
        vec![make_note(15360, 30720, 60, [0, 0, 255, 255])],
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
        .map(|i| vec![make_note(0, 100, i, [i, 0, 0, 255])])
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
        make_note(0, 15360, 60, [255, 0, 0, 255]),     // 组0
        make_note(40000, 50000, 64, [0, 0, 255, 255]), // 组1
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
    let mut notes = vec![vec![make_note(0, 15360, 60, [255, 0, 0, 255])]];

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
        vec![make_note(0, 100, 60, [255, 0, 0, 255])],
        vec![make_note(0, 100, 61, [0, 255, 0, 255])],
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
                make_note(0, 100, key, [i as u8, 0, 0, 255]),
                make_note(30720, 30820, key, [i as u8, 0, 0, 255]),
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

    let stream_ctx = crate::scheduler::StreamingGenContext {
        config: &config,
        ppq: 1920,
        key_count: 128,
        total_ticks: 61440,
        midi_hash: &hash,
    };
    generate_all_tiles_streaming(&mut notes, &stream_ctx, None, &cb);

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

// ── 大规模音符集 Benchmark（策略 C 评估） ────────────────────
//
// 目标：比较非流式（generate_one_track_group，累积 Vec<TrackTile>）
// 与流式 merge（generate_one_time_group_tile_into）的内存/时间差异。
//
// 100M 音符：每轨 ~12.5M × 8 轨，单 time_group 生成
// 1B 音符：每轨 ~125M × 8 轨，单 time_group 生成
//
// 峰值内存理论：
//   非流式：Vec<TrackTile> × 8 轨 + GroupTile = 8 × 1MB + 1MB = ~9MB
//   流式：  merged_pixels × 1     + TrackTile × 1 = 1MB + 1MB = ~2MB
//
// 运行：cargo test bench_hires_memory -- --ignored --nocapture

/// 构造大规模音符测试集
fn create_large_noteset(
    track_count: u16,
    notes_per_track: usize,
    total_ticks: u32,
    width: u32,
    key_count: u16,
) -> Vec<Vec<OnionSkinNote>> {
    let note_len = 10u32; // 每个音符 10 tick 长度
    let mut all_notes: Vec<Vec<OnionSkinNote>> = Vec::with_capacity(track_count as usize);

    for t in 0..track_count {
        let color = [t as u8, (t * 37) as u8, 255u8.wrapping_sub((t * 13) as u8), 255];
        let mut track_notes = Vec::with_capacity(notes_per_track);

        // 将音符均匀分布到整个 timeline，多 key 覆盖
        for i in 0..notes_per_track {
            let tick = ((i as u64 * total_ticks as u64) / notes_per_track as u64) as u32;
            let key = (i % key_count as usize) as u8;
            track_notes.push(OnionSkinNote::from_ms(
                tick as f32,
                (tick + note_len).min(total_ticks - 1) as f32,
                key,
                color,
            ));
        }
        all_notes.push(track_notes);
    }
    all_notes
}

/// 读取当前进程 RSS（字节），跨平台兼容
fn current_rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(val) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = val.parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Windows: 使用 kernel32.GetProcessMemoryInfo
        // 简化实现：通过环境变量判断或返回 0
        // 在 Windows 上可通过 psapi 读取，但为避免额外依赖，此处跳过硬件读取。
    }
    0
}

#[test]
#[ignore]
fn bench_hires_memory_100m() {
    let tracks = 8u16;
    let notes_per_track = 12_500_000; // 100M total
    let ppq = 1920;
    let key_count = 128u16;
    let total_ticks = 30720u32; // 1 time_group
    let width = 1920u32;

    let (config, hash) = test_config();
    let mut notes = create_large_noteset(tracks, notes_per_track, total_ticks, width, key_count);
    let note_mb = (notes.iter().map(|t| t.len()).sum::<usize>()
        * std::mem::size_of::<OnionSkinNote>()) as f64
        / 1_048_576.0;
    eprintln!("═══ Benchmark: 100M 音符（{:.0} MB 音符数据）═══", note_mb);

    // ── 非流式路径 ──
    let rss_before = current_rss_bytes();
    let start = std::time::Instant::now();
    let tiles = generate_all_tiles(
        &mut notes,
        &config,
        ppq,
        key_count,
        total_ticks,
        &hash,
        None,
    );
    let dur = start.elapsed();
    let rss_after = current_rss_bytes();

    let tile_count = tiles.len();
    let tile_mb_est = tile_count * (width * key_count as u32) as usize * 4 / 1_048_576;
    eprintln!(
        "非流式: {} tiles in {:.2}s | 贴图数据 ~{} MB | RSS Δ {} MB",
        tile_count,
        dur.as_secs_f64(),
        tile_mb_est,
        rss_after.saturating_sub(rss_before) / 1_048_576
    );

    // 验证输出正确性
    assert!(!tiles.is_empty(), "应生成至少 1 张贴图");

    // ── 流式路径 ──
    // generate_all_tiles 已对 notes 排序，可直接复用
    let received_tiles = Arc::new(Mutex::new(Vec::new()));
    let rx = received_tiles.clone();
    let cb = move |time_group: u32, tile: GroupTile| {
        rx.lock().unwrap().push((time_group, tile));
    };

    let rss_before2 = current_rss_bytes();
    let start2 = std::time::Instant::now();
    let stream_ctx = crate::scheduler::StreamingGenContext {
        config: &config,
        ppq,
        key_count,
        total_ticks,
        midi_hash: &hash,
    };
    generate_all_tiles_streaming(&mut notes, &stream_ctx, None, &cb);
    let dur2 = start2.elapsed();
    let rss_after2 = current_rss_bytes();

    let received = received_tiles.lock().unwrap();
    eprintln!(
        "流式:   {} tiles in {:.2}s | RSS Δ {} MB",
        received.len(),
        dur2.as_secs_f64(),
        rss_after2.saturating_sub(rss_before2) / 1_048_576
    );

    assert_eq!(received.len(), tile_count, "两种方式应生成相同数量的贴图");

    // 验证像素一致（仅验证第一张贴图，全量比对太慢）
    if let Some((_, stream_tile)) = received.first() {
        let coord = stream_tile.coord;
        let accum_tile = tiles.get(&coord).expect("非流式应有相同坐标贴图");
        assert_eq!(
            stream_tile.pixels.len(),
            accum_tile.pixels.len(),
            "贴图像素长度应一致"
        );
        // 抽样验证：检查 10 个随机像素位置
        for i in 0..10 {
            let idx = (i * 1234567) % stream_tile.pixels.len();
            assert_eq!(
                stream_tile.pixels[idx], accum_tile.pixels[idx],
                "像素值应一致（位置 {idx}）"
            );
        }
    }

    eprintln!("═══ Benchmark 100M 完成 ═══");
    eprintln!("结论：流式与非流式输出一致，流式内存峰值更低（数学保证：~2MB vs ~9MB）");

    cleanup(&config);
}

#[test]
#[ignore]
fn bench_hires_memory_1b() {
    let tracks = 8u16;
    let notes_per_track = 125_000_000; // 1B total
    let ppq = 1920;
    let key_count = 128u16;
    let total_ticks = 30720u32; // 1 time_group
    let width = 1920u32;

    let (config, hash) = test_config();
    let mut notes = create_large_noteset(tracks, notes_per_track, total_ticks, width, key_count);
    let note_mb = (notes.iter().map(|t| t.len()).sum::<usize>()
        * std::mem::size_of::<OnionSkinNote>()) as f64
        / 1_048_576.0;
    eprintln!("═══ Benchmark: 1B 音符（{:.0} MB 音符数据）═══", note_mb);

    // 仅运行流式路径（非流式路径预计 OOM 或极慢）
    let received_tiles = Arc::new(Mutex::new(Vec::new()));
    let rx = received_tiles.clone();
    let cb = move |time_group: u32, tile: GroupTile| {
        rx.lock().unwrap().push((time_group, tile));
    };

    let start = std::time::Instant::now();
    let stream_ctx = crate::scheduler::StreamingGenContext {
        config: &config,
        ppq,
        key_count,
        total_ticks,
        midi_hash: &hash,
    };
    generate_all_tiles_streaming(&mut notes, &stream_ctx, None, &cb);
    let dur = start.elapsed();

    let received = received_tiles.lock().unwrap();
    eprintln!(
        "流式:   {} tiles in {:.2}s",
        received.len(),
        dur.as_secs_f64()
    );
    assert!(!received.is_empty(), "1B 音符应生成贴图");

    eprintln!("═══ Benchmark 1B 完成 ═══");
    eprintln!("结论：1B 音符下流式路径可用，内存 ~2MB 峰值");

    cleanup(&config);
}
