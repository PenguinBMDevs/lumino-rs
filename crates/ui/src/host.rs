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

mod cache;
mod dialog;
mod editor_ops;
mod event;
mod render;
mod render_ctx;
pub mod types;
mod window_ctx;

use render_ctx::{RenderContext, WgpuResources};
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
    /// 滚动速度追踪器（用于 overscan 计算）
    pub(crate) scroll_tracker: render::note_worker::ScrollVelocityTracker,
    /// 洋葱皮概览贴图是否处于激活状态（MIDI 加载后置 true，关闭后置 false）
    pub(crate) onion_skin_active: bool,
}

impl Host {
    /// 创建渲染上下文和窗口上下文（三个构造函数的公共逻辑）
    fn create_render_and_window_context(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> (RenderContext, WindowContext) {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let font = create_font_from_config(ui_config);

        let note_renderer = lumino_gfx::NoteRenderer::new(&gfx.device, &gfx.queue, gfx.format);
        let grid_renderer = lumino_gfx::GridRenderer::new(&gfx.device, gfx.format);

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
        );

        (render_ctx, WindowContext::new(window))
    }

    /// 创建 Host 公共字段（三个构造函数的公共 Self 初始化）
    fn new_common_fields(
        render_ctx: RenderContext,
        window_ctx: WindowContext,
        root: root::Root,
    ) -> Self {
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
            scroll_tracker: render::note_worker::ScrollVelocityTracker::new(),
            onion_skin_active: false,
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
        let (render_ctx, window_ctx) =
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = if is_progress {
            root::Root::new_progress(&ui_config.theme)
        } else {
            root::Root::new(ui_config)
        };
        Self::new_common_fields(render_ctx, window_ctx, root)
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
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = root::Root::new_dialog(&ui_config.theme, dialog_type);
        Self::new_common_fields(render_ctx, window_ctx, root)
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
            Self::create_render_and_window_context(window, width, height, ui_config, gfx);
        let root = root::Root::new_settings_dialog(&ui_config.theme, ui_config);
        Self::new_common_fields(render_ctx, window_ctx, root)
    }

    /// 启用真正的分离渲染线程（新架构）
    ///
    /// 这会将所有 WGPU 渲染（音符、网格、键盘、标尺）从 UI 线程完全分离
    pub fn enable_separate_render_thread(&mut self) {
        if self.render_ctx.wgpu_render_thread.is_some() {
            return;
        }

        // 创建音符事件通道（tx 不再存储，由 render thread 持有 rx）
        let (_tx, rx) = std::sync::mpsc::channel();

        // 启动 WGPU 渲染线程
        match WgpuRenderThread::spawn(
            self.render_ctx.device.clone(),
            self.render_ctx.queue.clone(),
            self.render_ctx.format,
            rx,
            Arc::clone(&self.render_ctx.render_cache.note_instances_buffer),
        ) {
            Ok(thread) => {
                self.render_ctx.wgpu_render_thread = Some(thread);
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

    /// 启动洋葱皮概览贴图后台生成（MIDI 加载后调用）
    pub fn generate_onion_skin(
        &mut self,
        notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        duration_ms: u32,
        key_mode: lumino_gfx::KeyMode,
    ) {
        self.onion_skin_active = true;
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::GenerateOnionSkin {
                    notes,
                    duration_ms,
                    key_mode,
                },
            );
        }
    }

    /// 释放洋葱皮资源（关闭 MIDI 时调用）
    pub fn dispose_onion_skin(&mut self) {
        if !self.onion_skin_active {
            return;
        }
        self.onion_skin_active = false;
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::DisposeOnionSkin);
        }
    }

    /// 启动高精度洋葱皮贴图生成（MIDI 加载后与低精度一起调用）
    pub fn generate_hires_onion_skin(
        &mut self,
        notes: Vec<Vec<lumino_gfx::OnionSkinNote>>,
        ppq: u16,
        key_count: u16,
        total_ticks: u32,
        config: lumino_gfx::HiResConfig,
        midi_hash: String,
    ) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(
                lumino_gfx::render_thread::ControlCommand::GenerateHiResOnionSkin {
                    notes,
                    ppq,
                    key_count,
                    total_ticks,
                    config,
                    midi_hash,
                },
            );
        }
    }

    /// 释放高精度洋葱皮资源（关闭 MIDI 时调用）
    pub fn dispose_hires_onion_skin(&mut self) {
        if let Some(ref thread) = self.render_ctx.wgpu_render_thread {
            thread.send_control(lumino_gfx::render_thread::ControlCommand::DisposeHiResOnionSkin);
        }
    }

    /// 取出洋葱皮生成进度（runner 每帧调用并转发到进度窗口）
    pub fn drain_onion_progress(&self) -> Vec<(String, f32)> {
        self.render_ctx
            .wgpu_render_thread
            .as_ref()
            .map(|t| t.drain_onion_progress())
            .unwrap_or_default()
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
        // 将双缓冲容量写入 breakdown 的附加字段
        breakdown.note_instances_front_cap = front_cap;
        breakdown.note_instances_front_len = front_len;
        breakdown.note_instances_back_cap = back_cap;
        breakdown.note_instances_back_len = back_len;
        breakdown.note_instance_size = std::mem::size_of::<lumino_gfx::NoteInstance>() as usize;

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
