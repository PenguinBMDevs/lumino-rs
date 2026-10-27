//! 窗口初始化、尺寸变化、LOD 档位选择。

use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{Window, WindowLevel},
};

use super::App;
use crate::{bench::Bench, camera::Camera, render::band::LodMode};

impl App {
    pub(super) fn init_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("CPU ONIONSKIN — 熠星始芒 164M")
            .with_inner_size(LogicalSize::new(
                f64::from(self.window_width),
                f64::from(self.window_height),
            ))
            .with_min_inner_size(LogicalSize::new(960.0, 600.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("[fatal] 创建窗口失败: {error}");
                event_loop.exit();
                return;
            }
        };
        if self.bench_seconds.is_some() {
            // 基准窗口置顶并聚焦，避免被终端遮挡导致 DWM present 节拍抖动。
            window.set_window_level(WindowLevel::AlwaysOnTop);
            window.focus_window();
        }
        let context = match softbuffer::Context::new(Arc::clone(&window)) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("[fatal] softbuffer Context 失败: {error}");
                event_loop.exit();
                return;
            }
        };
        let mut surface = match softbuffer::Surface::new(&context, Arc::clone(&window)) {
            Ok(surface) => surface,
            Err(error) => {
                eprintln!("[fatal] softbuffer Surface 失败: {error}");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        if let Err(error) = surface.resize(
            NonZeroU32::new(width).unwrap_or(NonZeroU32::MIN),
            NonZeroU32::new(height).unwrap_or(NonZeroU32::MIN),
        ) {
            eprintln!("[fatal] softbuffer resize 失败: {error}");
            event_loop.exit();
            return;
        }
        let mut camera = Camera::new(
            width,
            height,
            window.scale_factor() as f32,
            self.timeline_ticks,
            self.ppq,
            self.initial_zoom,
        );
        if self.overview {
            let fit_x = camera.canvas_width() / camera.total_ticks.max(1) as f32;
            camera.zoom_x = fit_x.clamp(0.0002, 40.0);
            camera.zoom_y = (camera.canvas_height() / camera.key_count.max(1) as f32).max(1.0);
            camera.scroll_x = 0.0;
            camera.target_scroll_x = 0.0;
            camera.scroll_y = 0.0;
            self.initial_zoom = camera.zoom_x;
        }
        if let Some(seconds) = self.bench_seconds {
            let mut bench = Bench::new(&camera, &self.index, seconds);
            if self.overview {
                bench.set_start(0.0);
            }
            camera.scroll_x = bench.start_scroll();
            camera.target_scroll_x = bench.start_scroll();
            self.bench = Some(bench);
            self.frame_interval = Duration::ZERO;
            self.stats.reset();
        }
        self.renderer.resize(width, height);
        self.camera = Some(camera);
        self.window = Some(window.clone());
        self.presenter.init(context, surface);
        self.needs_redraw = true;
        self.idle_reported = false;
        self.next_frame = Instant::now();
        self.frame_valid = false;
        self.content_dirty = true;
        self.bench_ready = self.bench.is_none();
        window.request_redraw();
    }

    pub(super) fn handle_resize(&mut self, size: PhysicalSize<u32>) {
        let width = size.width.max(1);
        let height = size.height.max(1);
        let scale = self
            .window
            .as_ref()
            .map_or(1.0, |window| window.scale_factor()) as f32;
        if let Some(camera) = &mut self.camera {
            camera.resize(width, height, scale);
        }
        self.renderer.resize(width, height);
        self.presenter.resize(width, height);
        self.needs_redraw = true;
        self.next_frame = Instant::now();
        self.frame_valid = false;
        self.content_dirty = true;
    }

    pub(super) fn estimated_visible(&self, camera: &Camera) -> u64 {
        let (tick_lo, tick_hi) = camera.visible_tick_range();
        let estimate_all = self.index.estimate_visible(tick_lo, tick_hi);
        let (key_lo, key_hi) = camera.visible_key_range();
        let key_fraction = f64::from(key_hi.saturating_sub(key_lo) + 1) / 128.0;
        (estimate_all as f64 * key_fraction) as u64
    }

    pub(super) fn choose_mode(&self, camera: &Camera, force_motion: bool) -> LodMode {
        let estimate = self.estimated_visible(camera);
        let moving = force_motion || camera.is_moving();
        // 全曲总览：逐音符绘制 1.64 亿条不可能 80fps，切密度热力图。
        // 用"可见 key 比例"后的估算，避免只在 40 个键上放大时误判成全曲。
        if estimate > 6_000_000 {
            return LodMode::Overview;
        }
        if moving && estimate > 250_000 {
            LodMode::Motion
        } else {
            LodMode::Exact
        }
    }

    pub(super) fn choose_track_stride(&self, camera: &Camera, force_motion: bool) -> usize {
        if !(force_motion || camera.is_moving()) {
            return 1;
        }
        let estimate = self.estimated_visible(camera);
        if estimate <= 500_000 {
            1
        } else {
            ((estimate as f64 / 500_000.0).ceil() as usize).clamp(1, 4)
        }
    }
}
