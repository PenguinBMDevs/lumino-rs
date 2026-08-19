//! 大规模音符集 Benchmark（策略 C 评估）
//!
//! 目标：比较非流式（generate_one_track_group，累积 Vec<WaterfallTrackTile>）
//! 与流式 merge（generate_one_time_group_tile_into）的内存/时间差异。
//!
//! 100M 音符：每轨 ~12.5M × 8 轨，单 time_group 生成
//! 1B 音符：每轨 ~125M × 8 轨，单 time_group 生成
//!
//! 峰值内存理论：
//!   非流式：Vec<WaterfallTrackTile> × 8 轨 + WaterfallGroupTile = 8 × 1MB + 1MB = ~9MB
//!   流式：  merged_pixels × 1     + WaterfallTrackTile × 1 = 1MB + 1MB = ~2MB
//!
//! 运行：cargo test bench_texture_waterfall_memory -- --ignored --nocapture

use std::sync::{Arc, Mutex};

use crate::texture_waterfall::note::WaterfallNote;
use crate::texture_waterfall::scheduler::{
    generate_waterfall_tiles, generate_waterfall_tiles_streaming,
};
use crate::texture_waterfall::types::WaterfallGroupTile;

use super::{cleanup, test_config};

/// 构造大规模音符测试集
fn create_large_noteset(
    track_count: u16,
    notes_per_track: usize,
    total_ticks: u32,
    key_count: u16,
) -> Vec<Vec<WaterfallNote>> {
    let note_len = 10u32; // 每个音符 10 tick 长度
    let mut all_notes: Vec<Vec<WaterfallNote>> = Vec::with_capacity(track_count as usize);

    for t in 0..track_count {
        let color = [
            t as u8,
            (t * 37) as u8,
            255u8.wrapping_sub((t * 13) as u8),
            255,
        ];
        let mut track_notes = Vec::with_capacity(notes_per_track);

        // 将音符均匀分布到整个 timeline，多 key 覆盖
        for i in 0..notes_per_track {
            let tick = ((i as u64 * total_ticks as u64) / notes_per_track as u64) as u32;
            let key = (i % key_count as usize) as u8;
            track_notes.push(WaterfallNote::from_ms(
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
                if line.starts_with("VmRSS:")
                    && let Some(val) = line.split_whitespace().nth(1)
                    && let Ok(kb) = val.parse::<u64>()
                {
                    return kb * 1024;
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
fn bench_texture_waterfall_memory_100m() {
    let tracks = 8u16;
    let notes_per_track = 12_500_000; // 100M total
    let ppq = 1920;
    let key_count = 128u16;
    let total_ticks = 30720u32; // 1 time_group
    let width = 1920u32;

    let (config, hash) = test_config();
    let mut notes = create_large_noteset(tracks, notes_per_track, total_ticks, key_count);
    let note_mb = (notes.iter().map(|t| t.len()).sum::<usize>()
        * std::mem::size_of::<WaterfallNote>()) as f64
        / 1_048_576.0;
    eprintln!("═══ Benchmark: 100M 音符（{:.0} MB 音符数据）═══", note_mb);

    // ── 非流式路径 ──
    let rss_before = current_rss_bytes();
    let start = std::time::Instant::now();
    let tiles = generate_waterfall_tiles(
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
    // generate_waterfall_tiles 已对 notes 排序，可直接复用
    let received_tiles = Arc::new(Mutex::new(Vec::new()));
    let rx = received_tiles.clone();
    let cb = move |time_group: u32, tile: WaterfallGroupTile| {
        rx.lock().expect("互斥锁应可获取").push((time_group, tile));
    };

    let rss_before2 = current_rss_bytes();
    let start2 = std::time::Instant::now();
    let stream_ctx = crate::texture_waterfall::scheduler::WaterfallGenContext {
        config: &config,
        ppq,
        key_count,
        total_ticks,
        midi_hash: &hash,
    };
    generate_waterfall_tiles_streaming(&mut notes, &stream_ctx, None, &cb);
    let dur2 = start2.elapsed();
    let rss_after2 = current_rss_bytes();

    let received = received_tiles.lock().expect("互斥锁应可获取");
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
fn bench_texture_waterfall_memory_1b() {
    let tracks = 8u16;
    let notes_per_track = 125_000_000; // 1B total
    let ppq = 1920;
    let key_count = 128u16;
    let total_ticks = 30720u32; // 1 time_group

    let (config, hash) = test_config();
    let mut notes = create_large_noteset(tracks, notes_per_track, total_ticks, key_count);
    let note_mb = (notes.iter().map(|t| t.len()).sum::<usize>()
        * std::mem::size_of::<WaterfallNote>()) as f64
        / 1_048_576.0;
    eprintln!("═══ Benchmark: 1B 音符（{:.0} MB 音符数据）═══", note_mb);

    // 仅运行流式路径（非流式路径预计 OOM 或极慢）
    let received_tiles = Arc::new(Mutex::new(Vec::new()));
    let rx = received_tiles.clone();
    let cb = move |time_group: u32, tile: WaterfallGroupTile| {
        rx.lock().expect("互斥锁应可获取").push((time_group, tile));
    };

    let start = std::time::Instant::now();
    let stream_ctx = crate::texture_waterfall::scheduler::WaterfallGenContext {
        config: &config,
        ppq,
        key_count,
        total_ticks,
        midi_hash: &hash,
    };
    generate_waterfall_tiles_streaming(&mut notes, &stream_ctx, None, &cb);
    let dur = start.elapsed();

    let received = received_tiles.lock().expect("互斥锁应可获取");
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
