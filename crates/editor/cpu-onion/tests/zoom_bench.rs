//! 1080p 快速大范围缩放性能实测（手工 release 运行，默认 `#[ignore]`）。
//!
//! ```text
//! $env:LUMINO_CPU_ONION_BENCH_MIDI = "D:\BM-DATA\MIDI File\Paprika's Aua Ah Community Merge (FULL).mid"
//! cargo test --release -p lumino-cpu-onion --test zoom_bench -- --ignored --nocapture
//! ```
//!
//! 相机锚点 = 最密小节中心；`zoom_x` 在对数空间从全曲适配扫到 8 px/tick
//! 再回来（2 个来回），每帧都变焦，因此不会命中窄条增量路径，只测全量
//! Exact/Overview。`current_track = 0`，CPU 只画其余洋葱皮轨（生产语义）。

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use lumino_cpu_onion::{CanvasCamera, CpuOnionRenderer, DensityPyramid, TrackIndex, uses_overview};
use lumino_midi_loader::loader::load_parsed_midi;
use lumino_midi_model::MidiDocument;

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const KEY_COUNT: u16 = 128;
const ZOOM_Y: f32 = 12.0;
const ZOOM_HIGH: f32 = 8.0;
const SECONDS: u64 = 5;

fn bands() -> usize {
    std::env::var("LUMINO_CPU_ONION_BANDS")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(12)
        .clamp(1, 24)
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = (((sorted.len() - 1) as f64) * q).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn summarize(samples: &mut [f64]) -> (f64, f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    (
        quantile(samples, 0.50),
        quantile(samples, 0.99),
        samples[samples.len() - 1],
        mean,
    )
}

#[test]
#[ignore = "release 真实 MIDI 性能实测，需 LUMINO_CPU_ONION_BENCH_MIDI"]
fn zoom_bench_real_midi() {
    assert!(
        !cfg!(debug_assertions) || std::env::var_os("LUMINO_ALLOW_DEBUG_BENCH").is_some(),
        "必须在 release 下跑：cargo test --release ..."
    );
    let Some(path) = std::env::var_os("LUMINO_CPU_ONION_BENCH_MIDI").map(PathBuf::from) else {
        println!("SKIP: 未设置 LUMINO_CPU_ONION_BENCH_MIDI");
        return;
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let started = Instant::now();
    let parsed = runtime
        .block_on(load_parsed_midi(path, None))
        .expect("加载 MIDI");
    let document = parsed.document.expect("可编辑 MidiDocument");
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let track_count = document.track_count().max(1);
    let total_ticks = document.total_ticks().max(1);
    let measure_ticks = u32::from(document.division()).saturating_mul(4).max(1);
    let document = Arc::new(document);

    let started = Instant::now();
    let index = Arc::new(TrackIndex::build(&document));
    let index_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let density = DensityPyramid::build(&document, 8, KEY_COUNT);
    let density_ms = started.elapsed().as_secs_f64() * 1000.0;

    let bands = bands();
    let mut density = density;
    let mut renderer = CpuOnionRenderer::new(bands, track_count);
    renderer.resize(WIDTH, HEIGHT);
    let muted = vec![false; track_count];
    let center_tick =
        (index.densest_measure as f32 * measure_ticks as f32) + measure_ticks as f32 * 0.5;
    let fit = WIDTH as f32 / total_ticks as f32;
    let low = fit.clamp(0.0002, ZOOM_HIGH);
    let cycles = 2.0_f32;
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut snapshot_samples = Vec::new();
    let mut overview_frames = 0usize;
    let mut exact_frames = 0usize;
    let mut coverage_frames = 0usize;
    let mut last_visible = 0u64;

    while start.elapsed() < Duration::from_secs(SECONDS) {
        let progress = (start.elapsed().as_secs_f32() / SECONDS as f32).clamp(0.0, 1.0);
        let phase = progress * cycles * std::f32::consts::TAU;
        let t = 0.5 - 0.5 * phase.cos();
        let zoom = (low.ln() + (ZOOM_HIGH.ln() - low.ln()) * t).exp();
        let max_scroll = (total_ticks as f32 * zoom - WIDTH as f32).max(0.0);
        let scroll = (center_tick * zoom - WIDTH as f32 * 0.5).clamp(0.0, max_scroll);
        let camera = CanvasCamera {
            width: WIDTH,
            height: HEIGHT,
            scroll_x: scroll,
            scroll_y: 0.0,
            zoom_x: zoom,
            zoom_y: ZOOM_Y,
            key_count: KEY_COUNT,
        };
        let overview = uses_overview(&index, &camera);
        let coverage = density.can_render_coverage(&camera);
        if coverage {
            coverage_frames += 1;
        }
        let frame_started = Instant::now();
        renderer.render_full_with_density(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            0,
            &muted,
            Some(&density),
        );
        last_visible = renderer.visible_notes();
        // 模拟 UI 生产语义：整帧重绘后克隆像素 Arc（零拷贝共享）。
        // 下一次 render_full 在旧快照仍存活时由 Arc::make_mut 写时复制。
        let snapshot_started = Instant::now();
        let _snapshot: Arc<Vec<u8>> = Arc::clone(&renderer.frame().pixels);
        snapshot_samples.push(snapshot_started.elapsed().as_secs_f64() * 1000.0);
        samples.push(frame_started.elapsed().as_secs_f64() * 1000.0);
        if overview {
            overview_frames += 1;
        } else if !coverage {
            exact_frames += 1;
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    let frames = samples.len();
    let wall_fps = frames as f64 / elapsed.max(0.001);
    let (p50, p99, max, mean) = summarize(&mut samples);
    println!("== CPU 洋葱皮 1080p 快速缩放 ==");
    println!(
        "midi={track_count} tracks / {} notes | load {load_ms:.0} ms | index {index_ms:.0} ms | density {density_ms:.0} ms / {:.1} MiB | key_masks={:.1} MiB",
        index.total_notes,
        density.memory_bytes() as f64 / (1024.0 * 1024.0),
        index.key_mask_bytes() as f64 / (1024.0 * 1024.0)
    );
    println!(
        "frames={frames} | wall={wall_fps:.1} fps | render p50={p50:.2} p99={p99:.2} max={max:.2} mean={mean:.2} ms | equivalent p50={:.1} fps",
        1000.0 / p50.max(0.001)
    );
    println!(
        "modes: overview {overview_frames} / coverage {coverage_frames} / exact {exact_frames} | last visible={last_visible} | bands={bands}"
    );
    // 平均 FPS 是硬门槛：单看 p50 会把 2 秒级尖峰藏掉。
    let grade = if wall_fps < 100.0 {
        "BELOW_FLOOR"
    } else if p50 <= 5.0 {
        "IDEAL(200fps)"
    } else if p50 <= 1000.0 / 130.0 {
        "EXCELLENT(130fps)"
    } else if p50 <= 10.0 {
        "FLOOR(100fps)"
    } else {
        "BELOW_FLOOR"
    };
    println!(
        "target: wall {wall_fps:.1}fps {} | p50 {p50:.2}ms | 100fps {} | 130fps {} | 200fps {} => {grade}",
        if wall_fps >= 100.0 { "PASS" } else { "FAIL" },
        if wall_fps >= 100.0 && p50 <= 10.0 {
            "PASS"
        } else {
            "FAIL"
        },
        if wall_fps >= 100.0 && p50 <= 1000.0 / 130.0 {
            "PASS"
        } else {
            "FAIL"
        },
        if wall_fps >= 100.0 && p50 <= 5.0 {
            "PASS"
        } else {
            "FAIL"
        }
    );

    // UI 快照评估：生产路径逐帧克隆像素 Arc（零拷贝）；旧路径
    // `Arc::from(slice)` 整帧拷贝。区域/窄条下一次写回时做写时复制。
    let (snap_p50, snap_p99, snap_max, snap_mean) = summarize(&mut snapshot_samples);
    let dummy = vec![0u8; WIDTH as usize * HEIGHT as usize * 4];
    let copy_started = Instant::now();
    for _ in 0..5 {
        let _snapshot: Arc<[u8]> = Arc::from(dummy.as_slice());
    }
    let copy_ms = copy_started.elapsed().as_secs_f64() * 1000.0 / 5.0;
    println!(
        "UI snapshot: clone p50 {snap_p50:.3} p99 {snap_p99:.3} max {snap_max:.3} mean {snap_mean:.3} ms/帧 | old copy {copy_ms:.2} ms/帧"
    );

    // 单笔删除后的覆盖脏区重建成本（不再整建 310M）。
    if let Some(note) = document.track_notes(1).iter().next().copied() {
        let mut edited = MidiDocument::clone(&document);
        edited.remove_note(1, 0);
        let edited_index = index.with_changes(1, &[note], &[]);
        assert!(density.apply_note_range(note.start_tick, note.end_tick, u16::from(note.key), -1));
        let edit_started = Instant::now();
        let dirty_ok = density.rebuild_dirty_coverage(&edited, &edited_index);
        let edit_ms = edit_started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "edit dirty coverage: ok={dirty_ok} rebuild={edit_ms:.2} ms (note {}..{} key {})",
            note.start_tick, note.end_tick, note.key
        );
    }
    assert!(frames > 0, "至少渲染一帧");
}
