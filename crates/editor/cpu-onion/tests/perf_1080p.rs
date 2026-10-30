//! 1080p CPU 洋葱皮性能实测（**手工 release 运行，默认 `#[ignore]`，不进 CI**）。
//!
//! 运行方式（必须 `--release`，本文件不产生任何硬断言，避免 CI 抖动）：
//! ```text
//! cargo test --release -p lumino-cpu-onion --test perf_1080p -- --ignored --nocapture
//! ```
//!
//! 测量项：
//! - `render_full`：首帧（冷启动）+ 60 帧的 p50 / p99 / max；
//! - `render_region`：200×24 单音符编辑脏区的 p50 / p99 / max；
//! - `TrackIndex::build`：20 万 / 100 万音符的耗时（取 3 次最好值）。
//!
//! 数据分布：`TRACKS` 条轨道、音符 tick 均匀落在 `[0, TICK_SPAN)`、key 均匀落在
//! `[40, 120]`、每条长度 `NOTE_LEN`。相机 `zoom = [0.25 px/tick, 12 px/key]`、
//! 视口 1920×1080 → 可见 tick 窗口 7680（占 `TICK_SPAN` 的 64%），全部 key 都在
//! 视口内，因此约 64% 音符可见（不是「全部在视口外」的作弊分布）。
//!
//! band 数按生产 `ui::host::render::cpu_onion::state::worker_bands` 同规则取
//! `available_parallelism().clamp(1, 12)`（生产 `MAX_BANDS = 12`）。

use std::sync::Arc;
use std::time::Instant;

use lumino_cpu_onion::{CanvasCamera, CpuOnionRect, CpuOnionRenderer, TrackIndex};
use lumino_midi_model::{MidiDocument, NoteEvent};

/// 视口宽（1080p）。
const W: u32 = 1920;
/// 视口高（1080p）。
const H: u32 = 1080;
/// 键总数。
const KEY_COUNT: u16 = 128;
/// 相机缩放 `[zoom_x, zoom_y]`。
const ZOOM: [f32; 2] = [0.25, 12.0];
/// 单音符 tick 长度（= 12 px 宽）。
const NOTE_LEN: u32 = 48;
/// 音符 tick 分布跨度。
const TICK_SPAN: u32 = 12_000;
/// 轨道数。
const TRACKS: usize = 64;
/// 1080p 全帧测量用的音符总数。
const NOTES_FULL: usize = 200_000;
/// 采样帧数。
const FRAMES: usize = 60;
/// 单音符编辑脏区（x, y, w, h）。
const REGION: CpuOnionRect = CpuOnionRect::new(400, 300, 200, 24);
/// 无当前轨（洋葱皮画全部轨道，CPU 路径的最坏负载）。
const NO_CURRENT_TRACK: usize = usize::MAX;

/// 简单线性同余伪随机（确定性，不引入 rand 依赖）。
fn next_rand(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// 生成 `total` 个音符、`tracks` 条轨道的确定性语料（逐轨排序后批量插入）。
fn corpus(total: usize, tracks: usize) -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(tracks as u16, 480);
    let per_track = (total / tracks).max(1);
    let mut state = 0x243F_6A88_85A3_08D3u64;
    for track in 0..tracks {
        let mut notes = Vec::with_capacity(per_track);
        for _ in 0..per_track {
            let tick = (next_rand(&mut state) % u64::from(TICK_SPAN)) as u32;
            let key = 40 + (next_rand(&mut state) % 81) as u8;
            notes.push(NoteEvent::new(tick, tick + NOTE_LEN, key, 100, 0));
        }
        notes.sort_by_key(|note| note.start_tick);
        document.batch_insert_notes(track, notes);
    }
    document
}

/// 与生产同规则的 band 数。
fn worker_bands() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |parallelism| parallelism.get())
        .clamp(1, 12)
}

/// 相机（滚动 0、无键盘栏/标尺偏移）。
fn camera(scroll_x: f32) -> CanvasCamera {
    CanvasCamera {
        width: W,
        height: H,
        scroll_x,
        scroll_y: 0.0,
        zoom_x: ZOOM[0],
        zoom_y: ZOOM[1],
        key_count: KEY_COUNT,
    }
}

/// 与 `cull.wgsl` 同口径的「与视口相交」计数（报告可见音符数用）。
fn visible_notes(document: &MidiDocument, camera: &CanvasCamera) -> usize {
    let mut count = 0;
    for track in 0..document.notes.len() {
        for note in document.track_notes(track).iter() {
            let sx = note.start_tick as f32 * camera.zoom_x - camera.scroll_x;
            let sw = note.length() as f32 * camera.zoom_x;
            let sy = camera.key_to_y(note.key as u16);
            if sx <= W as f32 && sx + sw >= 0.0 && sy + camera.zoom_y >= 0.0 && sy <= H as f32 {
                count += 1;
            }
        }
    }
    count
}

/// 分位数（最近秩）。
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = (((sorted.len() - 1) as f64) * q).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// 排序后的样本摘要 `(p50, p99, max, mean)`（毫秒）。
fn summarize(samples: &mut [f64]) -> (f64, f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let p99 = quantile(samples, 0.99);
    let p50 = quantile(samples, 0.50);
    let max = samples[samples.len() - 1];
    (p50, p99, max, mean)
}

/// 打印样本原始值（10 个一行）。
fn print_samples(label: &str, samples: &[f64]) {
    for (row, chunk) in samples.chunks(10).enumerate() {
        let values: Vec<String> = chunk.iter().map(|ms| format!("{ms:.3}")).collect();
        println!("    {label}[{row}] {}", values.join(" "));
    }
}

/// 1080p 全帧 / 脏区 / 索引构建三段性能实测。
#[test]
#[ignore = "release 性能实测，手工运行（cargo test --release ... -- --ignored --nocapture）"]
fn perf_1080p_cpu_onion() {
    let bands = worker_bands();
    let cores = std::thread::available_parallelism().map_or(0, |p| p.get());
    println!("== CPU 洋葱皮 1080p 性能实测 ==");
    println!(
        "profile: debug_assertions={} | CPU 核数 {cores} | band 数 {bands} | 视口 {W}×{H} | zoom {:?}",
        cfg!(debug_assertions),
        ZOOM
    );
    assert!(
        !cfg!(debug_assertions) || std::env::var_os("LUMINO_ALLOW_DEBUG_BENCH").is_some(),
        "本实测必须在 release 下跑：cargo test --release -p lumino-cpu-onion --test perf_1080p \
         -- --ignored --nocapture（debug 数字没有参考价值）"
    );

    // ── 夹具 ────────────────────────────────────────────────────────────────
    let started = Instant::now();
    let document = corpus(NOTES_FULL, TRACKS);
    let corpus_ms = started.elapsed().as_secs_f64() * 1000.0;
    let document = Arc::new(document);

    let started = Instant::now();
    let index = Arc::new(TrackIndex::build(&document));
    let index_200k_ms = started.elapsed().as_secs_f64() * 1000.0;

    let base_camera = camera(0.0);
    let visible = visible_notes(&document, &base_camera);
    println!(
        "夹具：{} 轨 / {} 音符（可见 {}，{:.1}%）｜语料构建 {:.1} ms｜TrackIndex::build {:.2} ms",
        TRACKS,
        index.total_notes,
        visible,
        visible as f64 * 100.0 / index.total_notes.max(1) as f64,
        corpus_ms,
        index_200k_ms
    );

    let mut renderer = CpuOnionRenderer::new(bands, TRACKS);
    renderer.resize(W, H);

    // ── render_full：冷启动 + 60 帧 ─────────────────────────────────────────
    let started = Instant::now();
    renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &base_camera,
        NO_CURRENT_TRACK,
        &[],
    );
    let cold_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut full_samples = Vec::with_capacity(FRAMES);
    let mut full_visible = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES {
        // 每帧横向滚动 1px（真实滚动场景；避免固定输入带来的过度缓存命中）
        let camera = camera(frame as f32);
        let started = Instant::now();
        renderer.render_full(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            NO_CURRENT_TRACK,
            &[],
        );
        full_samples.push(started.elapsed().as_secs_f64() * 1000.0);
        full_visible.push(renderer.visible_notes());
    }
    let (full_p50, full_p99, full_max, full_mean) = summarize(&mut full_samples);
    println!(
        "render_full  ：冷启动 {cold_ms:.2} ms｜p50 {full_p50:.2} ms｜p99 {full_p99:.2} ms｜max {full_max:.2} ms｜mean {full_mean:.2} ms（{FRAMES} 帧）"
    );
    println!(
        "  可见音符（band 内绘制次数，跨 band 音符重复计数）：首帧 {}｜末帧 {}｜最小 {}｜最大 {}",
        full_visible.first().copied().unwrap_or(0),
        full_visible.last().copied().unwrap_or(0),
        full_visible.iter().min().copied().unwrap_or(0),
        full_visible.iter().max().copied().unwrap_or(0)
    );
    print_samples("full", &full_samples);

    // 第二组 60 帧（热态）：区分「频率/散热导致的帧间上升」与「逐帧残留增长」。
    let mut full_samples2 = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES {
        let camera = camera((FRAMES + frame) as f32);
        let started = Instant::now();
        renderer.render_full(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            NO_CURRENT_TRACK,
            &[],
        );
        full_samples2.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let (full2_p50, full2_p99, full2_max, full2_mean) = summarize(&mut full_samples2);
    println!(
        "render_full#2：p50 {full2_p50:.2} ms｜p99 {full2_p99:.2} ms｜max {full2_max:.2} ms｜mean {full2_mean:.2} ms（热态第 2 组 {FRAMES} 帧）"
    );
    print_samples("full#2", &full_samples2);

    // ── render_region：200×24 脏区 ──────────────────────────────────────────
    // 先整帧渲染一次，保证脏区路径在已填充的帧上工作（生产调用序）。
    renderer.render_full(
        Arc::clone(&document),
        Arc::clone(&index),
        &base_camera,
        NO_CURRENT_TRACK,
        &[],
    );
    let mut region_samples = Vec::with_capacity(FRAMES);
    for frame in 0..FRAMES {
        let camera = camera(frame as f32);
        let started = Instant::now();
        renderer.render_region(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            NO_CURRENT_TRACK,
            &[],
            REGION,
        );
        region_samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let (region_p50, region_p99, region_max, region_mean) = summarize(&mut region_samples);
    let full_bytes = u64::from(W) * u64::from(H) * 4;
    let region_bytes = u64::from(REGION.width) * u64::from(REGION.height) * 4;
    println!(
        "render_region：{}×{} 脏区 p50 {region_p50:.3} ms｜p99 {region_p99:.3} ms｜max {region_max:.3} ms｜mean {region_mean:.3} ms（{FRAMES} 帧）",
        REGION.width, REGION.height
    );
    print_samples("region", &region_samples);

    // ── 局部上传 vs 全量上传（字节数，计算值）───────────────────────────────
    println!(
        "上传字节（计算值，非实测）：全量 {full_bytes} B｜局部 {region_bytes} B｜比值 {:.1}×",
        full_bytes as f64 / region_bytes as f64
    );

    // ── TrackIndex::build 规模曲线 ──────────────────────────────────────────
    for total in [200_000usize, 1_000_000] {
        let started = Instant::now();
        let document = corpus(total, TRACKS);
        let corpus_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut best_ms = f64::MAX;
        let mut samples = Vec::with_capacity(3);
        for _ in 0..3 {
            let started = Instant::now();
            let built = TrackIndex::build(&document);
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            best_ms = best_ms.min(elapsed);
            samples.push(elapsed);
            assert_eq!(built.total_notes, (total / TRACKS) as u64 * TRACKS as u64);
        }
        println!(
            "TrackIndex::build：{} 音符 / {} 轨｜最好 {best_ms:.2} ms｜三次 {:?}｜语料构建 {corpus_ms:.1} ms",
            total,
            TRACKS,
            samples
                .iter()
                .map(|ms| format!("{ms:.2}"))
                .collect::<Vec<_>>()
        );
    }
}
