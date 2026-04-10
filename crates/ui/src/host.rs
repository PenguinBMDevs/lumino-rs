//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（iced UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能

use std::{sync::Arc, time::Instant};

use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use lumino_gfx::{GridLineInstance, GridRenderer, NoteInstance, NoteRenderer};

use iced_winit::runtime::user_interface::Cache;
use iced_winit::{Clipboard, winit};

use iced_core::{Font, Pixels, Size, mouse};

use crate::{config, root, settings, window};

mod dialog;
mod editor_ops;
mod event;
mod render;
pub mod types;

pub use types::{DialogResult, NoteData, TrackNotes};

/// 渲染缓存 - 避免每帧重复上传相同数据
pub struct RenderCache {
    /// 缓存的网格线实例
    pub grid_instances: Vec<GridLineInstance>,
    /// 缓存的音符实例
    pub note_instances: Vec<NoteInstance>,
    /// 网格线视口哈希（用于检测变化）
    pub grid_viewport_hash: u64,
    /// 音符视口哈希（用于检测变化）
    pub note_viewport_hash: u64,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            grid_instances: Vec::new(),
            note_instances: Vec::new(),
            grid_viewport_hash: 0,
            note_viewport_hash: 0,
        }
    }

    /// 计算视口状态的哈希值
    pub fn compute_viewport_hash(
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
        canvas_width: f32,
        canvas_height: f32,
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        scroll_x.to_bits().hash(&mut hasher);
        scroll_y.to_bits().hash(&mut hasher);
        zoom_x.to_bits().hash(&mut hasher);
        zoom_y.to_bits().hash(&mut hasher);
        canvas_width.to_bits().hash(&mut hasher);
        canvas_height.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
pub struct Host {
    pub(crate) window: Arc<winit::window::Window>,
    pub(crate) root: root::Root,
    pub(crate) renderer: Renderer,
    pub(crate) events: Vec<iced_core::Event>,
    pub(crate) cursor: mouse::Cursor,
    pub(crate) cache: Cache,
    pub(crate) clipboard: Clipboard,
    pub(crate) viewport: Viewport,
    pub(crate) pending_window_action: Option<window::TrafficAction>,
    pub(crate) pending_drag: bool,
    /// 逻辑光标位置（用于音符预览和触控拖动）
    pub cursor_position: Option<iced_core::Point>,
    pub(crate) last_fps_update: Instant,
    /// 帧计数器（用于 FPS 计算）
    pub(crate) frame_count: u32,
    /// 是否正在拖拽调整工具栏高度
    pub(crate) is_toolbar_resizing: bool,
    /// 是否跳过 Iced UI 渲染（用于性能测试）
    pub skip_ui_rendering: bool,
    /// 音符渲染器
    pub(crate) note_renderer: NoteRenderer,
    /// 网格渲染器
    pub(crate) grid_renderer: GridRenderer,
    /// 上一帧时间
    pub(crate) last_frame_time: Instant,
    /// iced UI 树是否需要重建（事件产生了状态变更时才为 true）
    pub(crate) ui_dirty: bool,
    /// 渲染缓存 - 避免重复上传数据
    pub(crate) render_cache: RenderCache,
    /// 上次渲染时的编辑状态（用于检测 preview / drawing 音符变化）
    pub(crate) last_edit_state: crate::editor::EditState,
    /// 上次渲染时的光标位置（用于检测 preview 音符变化）
    pub(crate) last_cursor_position: Option<iced_core::Point>,
}

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
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 根据配置创建字体
        let font = create_font_from_config(ui_config);

        // 初始化 iced 渲染器
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                iced_wgpu::graphics::Shell::headless(),
            );
            Renderer::new(engine, font, Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);
        // 创建 wgpu 网格渲染器
        let grid_renderer = GridRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: if is_progress {
                root::Root::new_progress(&ui_config.theme)
            } else {
                root::Root::new(ui_config)
            },
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            grid_renderer,
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
            skip_ui_rendering: false,
            ui_dirty: false,
            render_cache: RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
        }
    }

    /// 创建对话框 Host
    pub fn new_dialog(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 根据配置创建字体
        let font = create_font_from_config(ui_config);

        // 初始化 iced 渲染器
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                iced_wgpu::graphics::Shell::headless(),
            );
            Renderer::new(engine, font, Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);
        // 创建 wgpu 网格渲染器
        let grid_renderer = GridRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: root::Root::new_dialog(&ui_config.theme),
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            grid_renderer,
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
            skip_ui_rendering: false,
            ui_dirty: false,
            render_cache: RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
        }
    }

    /// 获取 root 引用
    pub fn root(&self) -> &root::Root {
        &self.root
    }

    /// 获取 root 可变引用
    pub fn root_mut(&mut self) -> &mut root::Root {
        &mut self.root
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 调整窗口大小
    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<iced_core::Point> {
        self.cursor_position
    }
}

/// 根据配置创建字体
///
/// 使用系统字体名称或默认字体
///
/// 注意：Font::with_name 需要 'static 字符串，
/// 我们使用 Box::leak 来创建一个静态字符串引用
fn create_font_from_config(ui_config: &config::UiConfig) -> Font {
    // 优先使用自定义字体路径
    if !ui_config.program_font_path.is_empty() {
        let path = std::path::Path::new(&ui_config.program_font_path);
        if path.exists() {
            tracing::info!("检测到自定义字体路径: {:?}", path);
            // 自定义字体文件加载需要重启应用才能生效
            // 这里只记录日志
        }
    }

    // 其次使用系统字体名称
    if !ui_config.program_font_name.is_empty() {
        // 将 String 转换为 'static str
        // Box::leak 会泄漏内存，但配置变更频率很低，这是可接受的权衡
        let static_name: &'static str =
            Box::leak(ui_config.program_font_name.clone().into_boxed_str());

        tracing::info!("应用字体: {}", ui_config.program_font_name);
        return Font::with_name(static_name);
    }

    // 使用默认字体
    tracing::info!("使用默认字体 (SansSerif)");
    Font::default()
}
