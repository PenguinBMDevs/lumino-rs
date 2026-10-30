//! 纯 CPU 洋葱皮钢琴卷帘渲染示例。
//!
//! 目标文件：熠星始芒 StarT 164M v1.1fix（164,206,462 音符 / 826 轨）。
//! 数据链路与主程序相同：`lumino_midi_loader::loader::load_parsed_midi`。

mod app;
mod bench;
mod camera;
mod edit;
mod edit_overlay;
mod model;
mod present;
mod render;
mod stats;
mod text;

use std::{fs::File, io::BufWriter, path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use clap::Parser;
use lumino_diagnostics::memtrace::TaggedAlloc;
use lumino_midi_loader::{
    NoteEvent,
    loader::{ProgressCallback, load_parsed_midi},
};

/// 全局分配器与主程序一致：LW 分配标签 + mimalloc 后端。
#[global_allocator]
static GLOBAL_ALLOC: TaggedAlloc = TaggedAlloc;

const DEFAULT_MIDI: &str =
    r"D:\熠星始芒：StarT+2023fix+v1.1\熠星始芒：StarT 164 Million v1.1fix.mid";

/// CPU ONIONSKIN 测试程序。
#[derive(Parser, Debug)]
#[command(name = "cpu-onionskin", about = "纯 CPU 洋葱皮钢琴卷帘渲染示例")]
struct Cli {
    /// MIDI 文件路径。
    #[arg(long, default_value = DEFAULT_MIDI)]
    midi: PathBuf,

    /// 窗口逻辑宽度。
    #[arg(long, default_value_t = 1600)]
    width: u32,

    /// 窗口逻辑高度。
    #[arg(long, default_value_t = 900)]
    height: u32,

    /// 初始横向缩放（像素 / tick）。
    #[arg(long, default_value_t = 0.1)]
    zoom: f32,

    /// tokio 渲染 worker 数（默认 = 物理核数）。
    #[arg(long)]
    workers: Option<usize>,

    /// 基准模式时长（秒）；指定后自动滚屏并输出 PASS/FAIL。
    #[arg(long)]
    bench: Option<u64>,

    /// 禁用 AVX2，强制标量路径（用于 A/B 对比）。
    #[arg(long)]
    no_simd: bool,

    /// 初始当前音轨（默认取第一条非空轨）。
    #[arg(long)]
    track: Option<usize>,

    /// 离屏渲染一帧到 PNG（用于检查音符矩形/风格），不打开窗口。
    #[arg(long)]
    screenshot: Option<PathBuf>,

    /// 交互冒烟：N 秒后自动退出（用于验证静止后不再刷新）。
    #[arg(long)]
    smoke: Option<u64>,

    /// 启动即全曲总览（配合 --bench 测密度热力图帧时间）。
    #[arg(long)]
    overview: bool,
}

fn print_progress(message: &str, progress: f64) {
    println!("[load] {:>6.2}%  {}", progress * 100.0, message);
}

fn main() -> Result<()> {
    #[cfg(target_os = "windows")]
    // SAFETY: timeBeginPeriod 是 Win32 媒体定时器 API，进程启动时调用一次；
    // 不涉及指针/所有权，失败也只会退回系统默认时钟粒度。
    unsafe {
        windows::Win32::Media::timeBeginPeriod(1);
    }
    let cli = Cli::parse();
    let workers = cli
        .workers
        .unwrap_or_else(|| physical_cores().max(1))
        .max(1);
    println!(
        "[cpu-onionskin] midi={} workers={} simd={}",
        cli.midi.display(),
        workers,
        if cli.no_simd { "scalar" } else { "auto" }
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .thread_name("cpu-onionskin-render")
        .build()
        .context("创建 tokio 渲染运行时失败")?;

    let load_start = Instant::now();
    let progress: ProgressCallback = Arc::new(print_progress);
    let parsed = runtime
        .block_on(load_parsed_midi(cli.midi.clone(), Some(&progress)))
        .with_context(|| format!("加载 MIDI 失败: {}", cli.midi.display()))?;
    let load_ms = load_start.elapsed().as_millis();

    let document = parsed
        .document
        .context("加载结果不含 MidiDocument（不是可编辑 MIDI？）")?;

    // 与主程序 MidiParsed 处理一致：加载后切 Random 调色板并锁定。
    let palette_ok = lumino_extras::palette::set_current_palette_by_name("Random");
    lumino_extras::palette::lock_palette();
    if !palette_ok {
        eprintln!("[warn] 未找到 Random 调色板，使用默认调色板");
    }

    let track_count = document.track_count();
    let current_track = cli.track.unwrap_or_else(|| {
        (0..track_count)
            .find(|&track| document.track_note_count(track as u16) > 0)
            .unwrap_or(0)
    });
    anyhow::ensure!(
        current_track < track_count,
        "当前音轨 {current_track} 超出范围（共 {track_count} 轨）"
    );
    let measure_ticks = (document.division() as u32).saturating_mul(4).max(1);
    let timeline_ticks = document
        .total_ticks()
        .saturating_add(measure_ticks.saturating_mul(100))
        .max(1);

    println!(
        "[load] done: notes={} tracks={} ticks={} division={} load_ms={} rss_mb={}",
        parsed.info.total_notes,
        track_count,
        document.total_ticks(),
        document.division(),
        load_ms,
        rss_mb()
    );
    println!(
        "[data] MidiDocument={} MiB (single authority, no render copy), NoteEvent={} B",
        (parsed.info.total_notes as usize * std::mem::size_of::<NoteEvent>()) / (1024 * 1024),
        std::mem::size_of::<NoteEvent>()
    );
    println!(
        "[data] current_track={} notes={}",
        current_track,
        document.track_note_count(current_track as u16)
    );

    let index_start = Instant::now();
    let index = model::TrackIndex::build(&document);
    println!(
        "[index] max_len/measure_counts built in {} ms; densest measure={} notes={}; long_notes={} ({} KiB)",
        index_start.elapsed().as_millis(),
        index.densest_measure,
        index.densest_measure_notes,
        index.long_note_count,
        index.long_note_count as usize * std::mem::size_of::<NoteEvent>() / 1024
    );

    let simd = render::simd::SimdLevel::detect(cli.no_simd);
    let style = Arc::new(render::color::NoteStyle::new(track_count));
    let index = Arc::new(index);

    if let Some(path) = &cli.screenshot {
        let mut renderer = render::Renderer::new(runtime, workers, Arc::clone(&style), simd);
        let width = cli.width.max(64);
        let height = cli.height.max(64);
        renderer.resize(width, height);
        let mut camera = camera::Camera::new(
            width,
            height,
            1.0,
            timeline_ticks,
            document.division(),
            cli.zoom,
        );
        let start_tick =
            (index.densest_measure.saturating_sub(1) as u32).saturating_mul(index.measure_ticks);
        camera.scroll_x = (start_tick as f32 * camera.zoom_x).clamp(0.0, camera.max_scroll_x());
        camera.target_scroll_x = camera.scroll_x;
        camera.center_on_key(60);
        let render_start = Instant::now();
        let stats = renderer.render(
            Arc::clone(&document),
            Arc::clone(&index),
            &camera,
            current_track,
            render::band::LodMode::Exact,
        );
        let font = text::TextRenderer::new();
        let hud = format!(
            "screenshot exact | notes {} | visible {}",
            parsed.info.total_notes, stats.visible_notes
        );
        {
            let frame_width = renderer.width() as usize;
            let frame = renderer.frame_mut();
            render::chrome::draw_keyboard(frame, frame_width, &camera, font.as_ref());
            render::chrome::draw_ruler(frame, frame_width, &camera, font.as_ref());
            render::chrome::draw_status(frame, frame_width, &camera, &hud, font.as_ref());
        }
        save_png(path, renderer.frame(), width, height)
            .with_context(|| format!("写入截图失败: {}", path.display()))?;
        println!(
            "[screenshot] {} ({}x{}, zoom={} px/tick, visible={}, render={} ms)",
            path.display(),
            width,
            height,
            camera.zoom_x,
            stats.visible_notes,
            render_start.elapsed().as_millis()
        );
        return Ok(());
    }

    let density_start = Instant::now();
    let density = render::density::DensityPyramid::build(&document, workers);
    println!(
        "[density] pyramid built in {} ms, {:.1} MiB",
        density_start.elapsed().as_millis(),
        density.memory_mib()
    );
    let config = app::AppConfig {
        document,
        index,
        density,
        current_track,
        width: cli.width,
        height: cli.height,
        zoom: cli.zoom,
        workers,
        simd,
        bench_seconds: cli.bench,
        smoke_seconds: cli.smoke,
        overview: cli.overview,
        style,
        runtime,
    };
    println!(
        "[start] window={}x{} zoom={} px/tick simd={}",
        config.width,
        config.height,
        config.zoom,
        simd.name()
    );
    let bench_pass = app::run(config)?;
    if bench_pass == Some(false) {
        std::process::exit(1);
    }
    Ok(())
}

fn rss_mb() -> u64 {
    lumino_diagnostics::memory_monitor::MemoryMonitor::global().current_rss() / (1024 * 1024)
}

fn save_png(path: &std::path::Path, frame: &[u32], width: u32, height: u32) -> Result<()> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let mut rgba = Vec::with_capacity(frame.len() * 4);
    for &pixel in frame {
        rgba.push(((pixel >> 16) & 0xFF) as u8);
        rgba.push(((pixel >> 8) & 0xFF) as u8);
        rgba.push((pixel & 0xFF) as u8);
        rgba.push(0xFF);
    }
    writer.write_image_data(&rgba)?;
    Ok(())
}

fn physical_cores() -> usize {
    sysinfo::System::physical_core_count().unwrap_or(1).max(1)
}
