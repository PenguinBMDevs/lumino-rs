//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（iced UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能
//!
//! 架构说明：
//! - UI线程（主线程）：处理事件、更新状态、生成渲染命令
//! - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染

use std::{sync::Arc, time::Instant};

use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use iced_winit::runtime::user_interface::Cache;
use iced_winit::{Clipboard, winit};
use iced_core::{Font, Pixels, Size, mouse};

use crate::{config, root, settings, window, RenderCommand, RenderThreadHandle, spawn_render_thread};
use crate::{RenderParams, WgpuRenderThread};
use lumino_gfx::{AtomicSwappableBuffer, NoteInstance, SwappableBuffer};

mod dialog;
mod editor_ops;
mod event;
mod render;
pub mod types;

pub use types::{DialogResult, NoteData, TrackNotes};

/// 渲染缓存 - 避免每帧重复上传相同数据
pub struct RenderCache {
    /// 缓存的网格线实例
    pub grid_instances: Vec<lumino_gfx::GridLineInstance>,
    /// 缓存的音符实例
    pub note_instances: Vec<lumino_gfx::NoteInstance>,
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
/// 
/// 线程模型：
/// - UI线程（主线程）：处理事件、更新状态、生成渲染命令
/// - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染
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
    pub(crate) note_renderer: lumino_gfx::NoteRenderer,
    /// 网格渲染器
    pub(crate) grid_renderer: lumino_gfx::GridRenderer,
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
    /// 渲染线程句柄（可选，用于多线程渲染模式）
    pub(crate) render_thread: Option<RenderThreadHandle>,
    /// 是否使用独立渲染线程
    pub(crate) use_render_thread: bool,
    /// 新的 WGPU 渲染线程（真正分离）
    pub(crate) wgpu_render_thread: Option<WgpuRenderThread>,
    /// 音符数据双缓冲（零拷贝共享）
    pub(crate) note_buffer: Option<AtomicSwappableBuffer<NoteInstance>>,
    /// 是否使用新的分离渲染架构
    pub(crate) use_separate_render_thread: bool,
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
        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, gfx.format);
        // 创建 wgpu 网格渲染器
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

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
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
            skip_ui_rendering: false,
            note_renderer,
            grid_renderer,
            ui_dirty: false,
            render_cache: RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
            render_thread: None,
            use_render_thread: false,
            wgpu_render_thread: None,
            note_buffer: None,
            use_separate_render_thread: false,
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
        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, gfx.format);
        // 创建 wgpu 网格渲染器
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

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
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
            skip_ui_rendering: false,
            note_renderer,
            grid_renderer,
            ui_dirty: false,
            render_cache: RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
            render_thread: None,
            use_render_thread: false,
            wgpu_render_thread: None,
            note_buffer: None,
            use_separate_render_thread: false,
        }
    }

    /// 启用独立渲染线程模式
    /// 
    /// 这会将WGPU渲染从UI线程分离到独立线程，提高UI响应性
    pub fn enable_render_thread(&mut self) {
        if self.render_thread.is_some() {
            return;
        }

        let (mut handle, receiver) = RenderThreadHandle::new();
        let stats = Arc::clone(&handle.stats);

        // 启动渲染线程
        let thread_handle = spawn_render_thread(receiver, stats);
        
        // 存储线程句柄
        handle.thread_handle = Some(thread_handle);

        self.render_thread = Some(handle);
        self.use_render_thread = true;

        tracing::info!("Host: Render thread enabled");
    }

    /// 禁用独立渲染线程模式
    pub fn disable_render_thread(&mut self) {
        if let Some(handle) = self.render_thread.take() {
            handle.shutdown();
            self.use_render_thread = false;
            tracing::info!("Host: Render thread disabled");
        }
    }

    /// 获取渲染线程统计信息
    pub fn render_stats(&self) -> Option<crate::RenderStats> {
        self.render_thread.as_ref().map(|h| h.stats())
    }

    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符数据双缓冲
        let note_buffer = Arc::new(SwappableBuffer::<NoteInstance>::new(100000));

        // 启动 WGPU 渲染线程
        match WgpuRenderThread::spawn(self.window.clone(), note_buffer.clone()) {
            Ok(thread) => {
                self.wgpu_render_thread = Some(thread);
                self.note_buffer = Some(note_buffer);
                self.use_separate_render_thread = true;
                tracing::info!("Host: Separate WGPU render thread enabled");
            }
            Err(e) => {
                tracing::error!("Host: Failed to start separate render thread: {}", e);
            }
        }
    }

    /// 禁用分离渲染线程
    pub fn disable_separate_render_thread(&mut self) {
        if let Some(thread) = self.wgpu_render_thread.take() {
            thread.shutdown();
            self.use_separate_render_thread = false;
            self.note_buffer = None;
            tracing::info!("Host: Separate WGPU render thread disabled");
        }
    }

    /// 获取分离渲染线程统计
    pub fn separate_render_stats(&self) -> Option<crate::WgpuRenderStats> {
        self.wgpu_render_thread.as_ref().map(|t| t.stats())
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

        // 通知渲染线程调整大小
        if let Some(ref handle) = self.render_thread {
            handle.send(RenderCommand::Resize { width, height });
        }
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<iced_core::Point> {
        self.cursor_position
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // 确保渲染线程正确关闭
        if self.render_thread.is_some() {
            self.disable_render_thread();
        }
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
