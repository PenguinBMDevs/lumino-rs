use std::sync::Arc;

use iced_core::Size;
use iced_wgpu::graphics::Viewport;
use iced_winit::Clipboard;
use iced_winit::runtime::user_interface::Cache;
use iced_winit::winit;

use crate::config;
use crate::host::Host;
use crate::root;

impl Host {
    /// 创建新的 Host
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let root = if is_progress {
            root::Root::new_progress(&ui_config.theme)
        } else {
            root::Root::new(ui_config)
        };
        Self::create_host(window, width, height, ui_config, gfx, root)
    }

    /// 创建对话框 Host
    pub fn new_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let root = root::Root::new_dialog(&ui_config.theme);
        Self::create_host(window, width, height, ui_config, gfx, root)
    }

    /// 创建 Host 的通用实现
    fn create_host(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        root: root::Root,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());
        let font = super::font::create_font_from_config(ui_config);
        let renderer = super::font::create_renderer(gfx, font);
        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format);
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

        let now = std::time::Instant::now();

        Self {
            // 核心组件
            window,
            root,
            renderer,
            // 输入状态
            events: Vec::new(),
            cursor: iced_core::mouse::Cursor::Unavailable,
            cursor_position: None,
            is_mouse_pressed: false,
            is_toolbar_resizing: false,
            pending_drag: false,
            pending_window_action: None,
            // 渲染状态
            cache: Cache::new(),
            clipboard,
            viewport,
            render_cache: super::cache::RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
            ui_dirty: false,
            has_rendered_ui: false,
            skip_ui_rendering: false,
            note_renderer,
            grid_renderer,
            render_thread: None,
            use_render_thread: false,
            wgpu_render_thread: None,
            note_events_tx: None,
            use_separate_render_thread: false,
            // 性能监控
            last_frame_time: now,
            last_fps_update: now,
            frame_count: 0,
            // WGPU资源
            device: gfx.device.clone(),
            queue: gfx.queue.clone(),
            format: gfx.format,
        }
    }
}
