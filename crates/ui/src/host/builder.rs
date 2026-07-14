//! 构造方法 —— 从 Host 拆出的构造函数
//!
//! 包含 Host 的构造逻辑：创建渲染/窗口上下文、公共字段初始化、各种构造函数变体。

use std::sync::Arc;
use std::time::Instant;

use iced_core::Size;
use iced_wgpu::graphics::Viewport;
use iced_winit::winit;

use crate::config;
use crate::root;
use crate::statusbar::performance::CpuMonitor;

use super::render::note_worker::ScrollVelocityTracker;
use super::render_ctx::WgpuResources;
use super::{Host, RenderContext, WindowContext};

impl Host {
    /// 创建渲染上下文和窗口上下文（三个构造函数的公共逻辑）
    ///
    /// `needs_renderers` 仅主窗口为 `true`；dialog/progress 等轻量窗口不需要
    /// 音符/网格渲染器，跳过创建可显著降低初始化开销。
    fn create_render_and_window_context(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        needs_renderers: bool,
    ) -> (RenderContext, WindowContext) {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let font = super::create_font_from_config(ui_config);

        let note_renderer = needs_renderers
            .then(|| lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format));
        let grid_renderer =
            needs_renderers.then(|| lumino_gfx::GridRenderer::new(&gfx.device, gfx.format));

        let wgpu_resources = WgpuResources {
            device: gfx.device.clone(),
            queue: gfx.queue.clone(),
            format: gfx.format,
            adapter: gfx.adapter.clone(),
        };
        let render_ctx = RenderContext::new(
            &wgpu_resources,
            viewport,
            note_renderer,
            grid_renderer,
            font,
            &window,
        );

        (render_ctx, WindowContext::new(window))
    }

    /// 创建 Host 公共字段（三个构造函数的公共 Self 初始化）
    ///
    /// 干净启动时即初始化默认高精度洋葱皮上下文，确保无 MIDI 文件时
    /// 编辑音符也能触发贴图生成。
    fn new_common_fields(
        render_ctx: RenderContext,
        window_ctx: WindowContext,
        root: root::Root,
        ui_config: &config::UiConfig,
    ) -> Self {
        let key_count = if ui_config.enable_256key { 256 } else { 128 };
        let hires_config = lumino_gfx::HiResConfig {
            enabled: ui_config.hires_onion_enabled,
            measures_per_group: ui_config.hires_measures_per_group,
            tile_width_px: ui_config.hires_tile_width_px,
            cooldown_secs: ui_config.hires_cooldown_secs,
            gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
            render_mode: lumino_gfx::HiResRenderMode::default(),
            group_tile_mem_limit_mb: crate::constants::memory::DEFAULT_GROUP_TILE_MEM_LIMIT_MB,
            cache_dir: lumino_gfx::HiResConfig::default().cache_dir,
        };
        let midi_hash = lumino_gfx::compute_midi_hash(b"empty-project");
        let ppq = lumino_core::view_state::DEFAULT_PPQ;
        let total_ticks = lumino_core::view_state::DEFAULT_TOTAL_TICKS;

        Self {
            render_ctx,
            window_ctx,
            root,
            events: Vec::new(),
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            skip_ui_rendering: false,
            ui_dirty: false,
            cpu_monitor: CpuMonitor::new(),
            last_gpu_frame_time_ms: 0.0,
            scroll_tracker: ScrollVelocityTracker::new(),
            hires_dirty_tracks: std::collections::HashSet::new(),
            hires_dirty_regions: std::collections::HashMap::new(),
            hires_last_edit: None,
            hires_config: Some(hires_config),
            hires_midi_hash: Some(midi_hash),
            hires_gen_info: Some((ppq, key_count, total_ticks)),
            hires_overlay_sent: false,
        }
    }

    /// 创建新的 Host
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let (render_ctx, window_ctx) = Self::create_render_and_window_context(
            window,
            width,
            height,
            ui_config,
            gfx,
            !is_progress,
        );
        let root = if is_progress {
            root::Root::new_progress(&ui_config.theme)
        } else {
            root::Root::new(ui_config)
        };
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }

    /// 创建对话框 Host
    pub fn new_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        dialog_type: crate::state::root_state::DialogType,
    ) -> Self {
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx, false);
        let root = root::Root::new_dialog(&ui_config.theme, dialog_type);
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }

    /// 创建设置对话框 Host（使用主窗口的配置）
    pub fn new_settings_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx, false);
        let root = root::Root::new_settings_dialog(&ui_config.theme, ui_config);
        Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
    }
}
