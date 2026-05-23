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

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use iced_core::{Font, Size};
use iced_wgpu::graphics::Viewport;
use iced_winit::winit;

use crate::statusbar::performance::CpuMonitor;
use crate::{WgpuRenderThread, config, root, settings};
use render::note_worker::NoteWorker;

mod cache;
mod dialog;
mod editor_ops;
mod event;
mod render;
mod render_ctx;
pub mod types;
mod window_ctx;

use render_ctx::RenderContext;
use window_ctx::WindowContext;

pub use cache::RenderCache;
pub use types::{DialogResult, NoteData, TrackNotes};

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
///
/// 线程模型：
/// - UI线程（主线程）：处理事件、更新状态、生成渲染命令
/// - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染
///
/// 架构拆分：
/// - `render_ctx`: 渲染上下文（渲染器、GPU资源、渲染线程）
/// - `window_ctx`: 窗口上下文（窗口句柄、光标、剪贴板）
/// - 直连字段：框架/全局状态（root、events、帧统计等）
pub struct Host {
    /// 渲染上下文
    pub(crate) render_ctx: RenderContext,
    /// 窗口上下文
    pub(crate) window_ctx: WindowContext,
    /// 应用状态根节点
    pub(crate) root: root::Root,
    /// 事件列表
    pub(crate) events: Vec<iced_core::Event>,
    /// 上一帧时间
    pub(crate) last_frame_time: Instant,
    /// 上次 FPS 更新时间
    pub(crate) last_fps_update: Instant,
    /// 帧计数器
    pub(crate) frame_count: u32,
    /// 跳过 Iced UI 渲染（性能测试用）
    pub skip_ui_rendering: bool,
    /// UI 脏标记
    pub(crate) ui_dirty: bool,
    /// CPU 使用率监控器
    pub(crate) cpu_monitor: CpuMonitor,
    /// 上一次 GPU 帧耗时（ms）
    pub(crate) last_gpu_frame_time_ms: f32,
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

        // 根据配置创建字体
        let font = create_font_from_config(ui_config);

        // 创建 wgpu 音符 + 网格渲染器
        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format);
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

        let render_ctx = RenderContext::new(
            gfx.device.clone(),
            gfx.queue.clone(),
            gfx.format,
            &gfx.adapter,
            viewport,
            note_renderer,
            grid_renderer,
            font,
        );

        Self {
            render_ctx,
            window_ctx: WindowContext::new(window),
            root: if is_progress {
                root::Root::new_progress(&ui_config.theme)
            } else {
                root::Root::new(ui_config)
            },
            events: Vec::new(),
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            skip_ui_rendering: false,
            ui_dirty: false,
            cpu_monitor: CpuMonitor::new(),
            last_gpu_frame_time_ms: 0.0,
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

        let font = create_font_from_config(ui_config);

        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format);
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

        let render_ctx = RenderContext::new(
            gfx.device.clone(),
            gfx.queue.clone(),
            gfx.format,
            &gfx.adapter,
            viewport,
            note_renderer,
            grid_renderer,
            font,
        );

        Self {
            render_ctx,
            window_ctx: WindowContext::new(window),
            root: root::Root::new_dialog(&ui_config.theme),
            events: Vec::new(),
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            skip_ui_rendering: false,
            ui_dirty: false,
            cpu_monitor: CpuMonitor::new(),
            last_gpu_frame_time_ms: 0.0,
        }
    }

    /// 确保 NoteWorker 已创建（懒加载）
    ///
    /// NoteWorker 用于将音符实例构建从主线程卸载到独立线程。
    /// 在两种渲染模式下都会使用：
    /// - 分离线程模式：非阻塞 fire-and-forget
    /// - 单线程模式：dispatch + 同步等待（仍能在本帧内并行化）
    fn ensure_note_worker(&mut self) {
        if self.render_ctx.note_worker.is_none() {
            self.render_ctx.note_worker = Some(NoteWorker::spawn());
            tracing::info!("NoteWorker: spawned");
        }
    }

    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.render_ctx.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符事件通道
        let (tx, rx) = std::sync::mpsc::channel();

        // 启动 WGPU 渲染线程
        match WgpuRenderThread::spawn(
            self.render_ctx.device.clone(),
            self.render_ctx.queue.clone(),
            self.render_ctx.format,
            rx,
            Arc::clone(&self.render_ctx.render_cache.note_instances_buffer),
            Some(Arc::clone(&self.render_ctx.render_cache.onion_note_buffer)),
        ) {
            Ok(thread) => {
                self.render_ctx.wgpu_render_thread = Some(thread);
                self.render_ctx.note_events_tx = Some(tx);
                self.render_ctx.use_separate_render_thread = true;
                tracing::info!("Host: Separate WGPU render thread enabled");
            }
            Err(e) => {
                tracing::error!("Host: Failed to start separate render thread: {}", e);
            }
        }
    }

    /// 禁用分离渲染线程
    pub fn disable_separate_render_thread(&mut self) {
        if let Some(thread) = self.render_ctx.wgpu_render_thread.take() {
            thread.shutdown();
            self.render_ctx.use_separate_render_thread = false;
            self.render_ctx.note_events_tx = None;
            tracing::info!("Host: Separate WGPU render thread disabled");
        }
    }

    /// 获取分离渲染线程统计
    pub fn separate_render_stats(&self) -> Option<crate::WgpuRenderStats> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.stats())
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
        self.render_ctx.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window_ctx.window.scale_factor() as f32,
        );
    }

    /// 获取当前光标位置（逻辑坐标）
    pub fn cursor_position(&self) -> Option<iced_core::Point> {
        self.window_ctx.cursor_position
    }

    /// 收集所有组件的内存占用快照（Root + RenderCache）
    pub fn memory_breakdown(&self) -> root::MemoryBreakdown {
        let mut breakdown = self.root.memory_breakdown();

        // 从 RenderCache 获取主音符双缓冲容量
        let (front_cap, front_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .front_info();
        let (back_cap, back_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .back_info();
        // 洋葱皮双缓冲容量
        let (onion_front_cap, onion_front_len) =
            self.render_ctx.render_cache.onion_note_buffer.front_info();
        let (onion_back_cap, onion_back_len) =
            self.render_ctx.render_cache.onion_note_buffer.back_info();
        let note_size = std::mem::size_of::<lumino_gfx::OnionNote>() as u64;

        tracing::debug!(
            "MemoryBreakdown: note front(cap={}, len={}) back(cap={}, len={}) onion front(cap={}, len={}) back(cap={}, len={}) note_size={}",
            front_cap,
            front_len,
            back_cap,
            back_len,
            onion_front_cap,
            onion_front_len,
            onion_back_cap,
            onion_back_len,
            note_size
        );

        // 将双缓冲容量写入 breakdown 的附加字段
        breakdown.note_instances_front_cap = front_cap;
        breakdown.note_instances_front_len = front_len;
        breakdown.note_instances_back_cap = back_cap;
        breakdown.note_instances_back_len = back_len;
        breakdown.note_instance_size = std::mem::size_of::<lumino_gfx::NoteInstance>() as usize;
        breakdown.onion_note_front_cap = onion_front_cap;
        breakdown.onion_note_front_len = onion_front_len;
        breakdown.onion_note_back_cap = onion_back_cap;
        breakdown.onion_note_back_len = onion_back_len;

        breakdown
    }
}

/// 字体名称缓存 —— OnceLock 确保只泄漏一次，而不是每次重绘都泄漏
static FONT_NAME_CACHE: OnceLock<String> = OnceLock::new();

/// 根据配置创建字体
///
/// 使用系统字体名称或默认字体
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
        let cached = FONT_NAME_CACHE.get_or_init(|| ui_config.program_font_name.clone());

        tracing::info!("应用字体: {}", cached);
        return Font::with_name(cached.as_str());
    }

    // 使用默认字体
    tracing::info!("使用默认字体 (SansSerif)");
    Font::default()
}
