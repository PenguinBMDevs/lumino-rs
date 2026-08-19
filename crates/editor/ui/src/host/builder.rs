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
use crate::root::handlers;
use crate::statusbar::performance::CpuMonitor;

use super::render_ctx::WgpuResources;
use super::{Host, RenderContext, WindowContext};

impl Host {
    /// 创建渲染上下文和窗口上下文（三个构造函数的公共逻辑）
    ///
    /// `needs_renderers` 仅主窗口为 `true`；dialog/progress 等轻量窗口不需要
    /// 音符/网格渲染器，跳过创建可显著降低初始化开销。
    ///
    /// `use_shared_engine` 为 `true` 时复用对话框共享的 iced Engine；
    /// 主窗口 / progress 窗口保持独立 Engine 以保留 WindowNotifier。
    fn create_render_and_window_context(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        needs_renderers: bool,
        use_shared_engine: bool,
    ) -> (RenderContext, WindowContext) {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let font = super::create_font_from_config(ui_config);

        let note_renderer = needs_renderers
            .then(|| lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format));

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
            font,
            &window,
            use_shared_engine,
        );

        (render_ctx, WindowContext::new(window))
    }

    /// 创建 Host 公共字段（三个构造函数的公共 Self 初始化）
    ///
    /// 干净启动时即初始化默认贴图瀑布流瀑布流上下文，确保无 MIDI 文件时
    /// 编辑音符也能触发贴图生成。
    fn new_common_fields(
        render_ctx: RenderContext,
        window_ctx: WindowContext,
        root: root::Root,
        ui_config: &config::UiConfig,
    ) -> Self {
        let key_count = if ui_config.enable_256key { 256 } else { 128 };
        let waterfall_config = lumino_gfx::TextureWaterfallConfig {
            enabled: ui_config.hires_onion_enabled,
            measures_per_group: ui_config.hires_measures_per_group,
            tile_width_px: ui_config.hires_tile_width_px,
            cooldown_secs: ui_config.hires_cooldown_secs,
            gpu_mem_limit_mb: ui_config.hires_gpu_mem_limit_mb,
            render_mode: lumino_gfx::TextureWaterfallRenderMode::default(),
            group_tile_mem_limit_mb: crate::constants::memory::DEFAULT_GROUP_TILE_MEM_LIMIT_MB,
            cache_dir: lumino_gfx::TextureWaterfallConfig::default().cache_dir,
        };
        let midi_hash = lumino_gfx::compute_waterfall_cache_hash(b"empty-project");
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
            waterfall_dirty_tracks: std::collections::HashSet::new(),
            waterfall_config: Some(waterfall_config),
            waterfall_midi_hash: Some(midi_hash),
            waterfall_gen_info: Some((ppq, key_count, total_ticks)),
            message_router: handlers::create_message_router(),
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
            false,
        );
        let root = if is_progress {
            root::Root::new_progress(&ui_config.theme, ui_config)
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
        let (render_ctx, window_ctx) = Self::create_render_and_window_context(
            window, width, height, ui_config, gfx, false, true,
        );
        let root = {
            puffin::profile_scope!("dialog_root_new");
            root::Root::new_dialog_with_config(&ui_config.theme, dialog_type, ui_config)
        };
        {
            puffin::profile_scope!("dialog_common_fields");
            Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
        }
    }

    /// 创建设置对话框 Host（使用主窗口的配置）
    pub fn new_settings_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let (render_ctx, window_ctx) = Self::create_render_and_window_context(
            window, width, height, ui_config, gfx, false, true,
        );
        let root = {
            puffin::profile_scope!("settings_root_new");
            root::Root::new_settings_dialog(&ui_config.theme, ui_config)
        };
        {
            puffin::profile_scope!("settings_common_fields");
            Self::new_common_fields(render_ctx, window_ctx, root, ui_config)
        }
    }
}
