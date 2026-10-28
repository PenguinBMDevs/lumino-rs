//! winit + softbuffer 窗口与帧循环。
//!
//! 静止时不再 request_redraw（`ControlFlow::Wait`），终端打印一次 IDLE；
//! 滚动/缩放动画期间按 144Hz 上限节流；`--bench` 用 Poll 跑密集段。
//!
//! 子模块：
//! - `window`：窗口/相机初始化、缩放档位选择
//! - `frame`：全量帧、滚动增量帧、编辑拖动预览
//! - `events`：鼠标/键盘输入、编辑操作、事件循环等待

mod canvas;
mod events;
mod frame;
mod handler;
mod present_bridge;
#[cfg(test)]
mod scroll_tests;
mod window;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use lumino_midi_loader::MidiDocument;
use winit::{
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::Window,
};

use crate::{
    bench::Bench,
    camera::Camera,
    edit::Editor,
    model::TrackIndex,
    present::Presenter,
    render::{Renderer, band::LodMode, color::NoteStyle, density::DensityPyramid, simd::SimdLevel},
    stats::PerfMonitor,
    text::TextRenderer,
};

/// 应用配置。
pub struct AppConfig {
    /// MIDI 文档（与编辑层共享同一个 Arc）。
    pub document: Arc<MidiDocument>,
    /// 聚合索引。
    pub index: Arc<TrackIndex>,
    /// 当前音轨。
    pub current_track: usize,
    /// 窗口逻辑宽高。
    pub width: u32,
    pub height: u32,
    /// 初始横向缩放（px/tick）。
    pub zoom: f32,
    /// tokio worker 数。
    pub workers: usize,
    /// SIMD 档位。
    pub simd: SimdLevel,
    /// 基准模式时长（秒）。
    pub bench_seconds: Option<u64>,
    /// 交互冒烟时长（秒）。
    pub smoke_seconds: Option<u64>,
    /// 全曲总览起始。
    pub overview: bool,
    /// 每轨颜色。
    pub style: Arc<NoteStyle>,
    /// 全曲密度金字塔。
    pub density: DensityPyramid,
    /// 渲染运行时。
    pub runtime: tokio::runtime::Runtime,
}

/// 运行事件循环；返回基准结果（`Some(false)` = 未达标）。
pub fn run(config: AppConfig) -> Result<Option<bool>> {
    let event_loop = EventLoop::new().context("创建 winit 事件循环失败")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(config);
    event_loop.run_app(&mut app).context("winit 事件循环失败")?;
    Ok(app.bench_pass)
}

/// 应用状态。
struct App {
    document: Arc<MidiDocument>,
    index: Arc<TrackIndex>,
    density: DensityPyramid,
    current_track: usize,
    workers: usize,
    editor: Editor,
    drag_base: Vec<u32>,
    overview_buf: Vec<u32>,
    renderer: Renderer,
    font: Option<TextRenderer>,
    window: Option<Arc<Window>>,
    presenter: crate::present::Presenter,
    camera: Option<Camera>,
    initial_zoom: f32,
    window_width: u32,
    window_height: u32,
    timeline_ticks: u32,
    ppq: u16,
    bench: Option<Bench>,
    bench_seconds: Option<u64>,
    smoke_deadline: Option<Instant>,
    overview: bool,
    bench_pass: Option<bool>,
    bench_reported: bool,
    stats: PerfMonitor,
    mouse: (f64, f64),
    modifiers: ModifiersState,
    needs_redraw: bool,
    idle_reported: bool,
    next_frame: Instant,
    frame_interval: Duration,
    last_frame: Instant,
    last_frame_start: Instant,
    frame_index: u64,
    frame_valid: bool,
    content_dirty: bool,
    last_scroll_x: f32,
    last_scroll_y: f32,
    last_zoom_x: f32,
    last_zoom_y: f32,
    last_width: u32,
    last_height: u32,
    last_path: &'static str,
    baseline_mode: LodMode,
    bench_ready: bool,
    bench_warmup_until: Option<Instant>,
    fps_ema: f32,
    last_work_ms: f32,
}

impl App {
    fn new(config: AppConfig) -> Self {
        let font = TextRenderer::new();
        if font.is_none() {
            eprintln!("[warn] 内置字体解析失败：标尺/状态栏文本将缺失");
        }
        let band_count = config.workers.max(2);
        let renderer = Renderer::new(
            config.runtime,
            band_count,
            Arc::clone(&config.style),
            config.simd,
        );
        let measure_ticks = (config.document.division() as u32).saturating_mul(4).max(1);
        let timeline_ticks = config
            .document
            .total_ticks()
            .saturating_add(measure_ticks.saturating_mul(100))
            .max(1);
        let ppq = config.document.division();
        let now = Instant::now();
        Self {
            document: config.document,
            index: config.index,
            density: config.density,
            current_track: config.current_track,
            workers: config.workers,
            editor: Editor::new(u32::from(ppq)),
            drag_base: Vec::new(),
            overview_buf: Vec::new(),
            renderer,
            font,
            window: None,
            presenter: Presenter::new(),
            camera: None,
            initial_zoom: config.zoom,
            window_width: config.width,
            window_height: config.height,
            timeline_ticks,
            ppq,
            bench: None,
            bench_seconds: config.bench_seconds,
            smoke_deadline: config
                .smoke_seconds
                .map(|seconds| now + Duration::from_secs(seconds.max(1))),
            overview: config.overview,
            bench_pass: None,
            bench_reported: false,
            stats: PerfMonitor::new(4096, config.workers),
            mouse: (0.0, 0.0),
            modifiers: ModifiersState::empty(),
            needs_redraw: true,
            idle_reported: false,
            next_frame: now,
            frame_interval: Duration::from_secs_f64(1.0 / 144.0),
            last_frame: now,
            last_frame_start: now,
            frame_index: 0,
            frame_valid: false,
            content_dirty: true,
            last_scroll_x: 0.0,
            last_scroll_y: 0.0,
            last_zoom_x: 0.0,
            last_zoom_y: 0.0,
            last_width: 0,
            last_height: 0,
            last_path: "none",
            baseline_mode: LodMode::Exact,
            bench_ready: false,
            bench_warmup_until: None,
            fps_ema: 0.0,
            last_work_ms: 0.0,
        }
    }
}
