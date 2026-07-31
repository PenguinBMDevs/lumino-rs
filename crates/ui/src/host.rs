//! Host 模块 - UI 宿主，管理渲染和事件处理
//!
//! 该模块已拆分为以下子模块：
//! - `types`: 类型定义和工具函数
//! - `render`: 渲染逻辑（iced UI 和 wgpu 音符）
//! - `event`: 事件处理（窗口事件、输入）
//! - `editor_ops`: 编辑器操作（音符、洋葱皮）
//! - `dialog`: 对话框和协作功能
//! - `builder`: 构造方法（创建渲染/窗口上下文、初始化公共字段）
//! - `hires`: 高精度贴图（生成、重生成、脏区域追踪、冷静期控制）
//! - `render_thread`: 分离渲染线程管理（启停、事件通道、统计）
//!
//! 架构说明：
//! - UI线程（主线程）：处理事件、更新状态、生成渲染命令
//! - 渲染线程（独立线程）：接收命令、管理GPU资源、执行实际渲染

use std::sync::OnceLock;
use std::time::Instant;

use iced_core::{Font, Size};
use iced_wgpu::Engine;
use iced_wgpu::graphics::Viewport;

use crate::statusbar::performance::CpuMonitor;
use crate::{config, message, root, settings};

mod builder;
mod cache;
mod dialog;
mod editor_ops;
mod event;
mod hires;
mod render;
mod render_ctx;
mod render_thread;
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
    /// 滚动速度追踪器（用于 overscan 计算）
    pub(crate) scroll_tracker: render::note_worker::ScrollVelocityTracker,
    /// 高精度贴图：有脏标记的音轨集合（编辑后需重生成）
    pub(crate) hires_dirty_tracks: std::collections::HashSet<u16>,
    /// 高精度贴图：脏区域追踪（track_idx → 脏音符列表），用于临时贴图覆层
    pub(crate) hires_dirty_regions: std::collections::HashMap<u16, Vec<lumino_gfx::OnionSkinNote>>,
    /// 高精度贴图：每个脏音轨受影响的 time_group 集合
    ///
    /// 仅用于 `ShowHiResDirtyOverlay` 命令过滤覆层范围，避免覆盖未编辑的
    /// time_group 导致原洋葱皮贴图被空白覆层盖住。
    /// `RegenerateHiResTrack` 全量重生，不使用此字段。
    pub(crate) hires_dirty_time_groups:
        std::collections::HashMap<u16, std::collections::HashSet<u32>>,
    /// 高精度贴图：最后一次编辑时间（用于冷静期判断）
    pub(crate) hires_last_edit: Option<Instant>,
    /// 高精度贴图：全量配置（重生成时直接使用副本）
    pub(crate) hires_config: Option<lumino_gfx::HiResConfig>,
    /// 高精度贴图：生成时的 MIDI 哈希（重生成时复用缓存分桶）
    pub(crate) hires_midi_hash: Option<String>,
    /// 高精度贴图：生成时的 (ppq, key_count, total_ticks)（重生成时复用）
    pub(crate) hires_gen_info: Option<(u16, u16, u32)>,
    /// 高精度贴图：脏区域覆层是否已发送到渲染线程（防止每帧重复发送）
    pub(crate) hires_overlay_sent: bool,
    /// 消息路由器（分发消息到各处理器）
    pub(crate) message_router: root::handlers::MessageRouter,
}

impl Host {
    /// 预加载所有音轨音符到 track_notes 缓存
    ///
    /// MIDI 加载后立即调用，确保后续重生成时能从缓存取到完整音轨数据，
    /// 避免预生成贴图被不完整数据覆盖。
    pub fn preload_track_notes(&mut self, track_notes: Vec<Vec<lumino_core::Note>>) {
        let editor_data = &mut self.root.editor.editor_state.data;
        for (track_idx, notes) in track_notes.into_iter().enumerate() {
            editor_data.track_notes.insert(track_idx, notes.into());
        }
        editor_data.track_notes_gen += 1;
        tracing::info!(
            "[onion-dirty] 预加载 track_notes: {} 个音轨",
            editor_data.track_notes.len()
        );
    }

    /// 获取 root 引用
    pub fn root(&self) -> &root::Root {
        &self.root
    }

    /// 获取 root 可变引用
    pub fn root_mut(&mut self) -> &mut root::Root {
        &mut self.root
    }

    /// 获取当前侧边栏音轨数量（用于推断高精度贴图音轨组范围）
    pub fn track_count(&self) -> usize {
        self.root.sidebar.tracks.len()
    }

    /// 获取设置面板引用
    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 获取当前 wgpu 纹理格式（用于视频导出时匹配 ffmpeg `-pix_fmt`）
    pub fn texture_format(&self) -> lumino_gfx::TextureFormat {
        self.render_ctx.format
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

    /// 标记 UI 脏，强制下一帧重绘时重建界面（而非使用渲染缓存）。
    ///
    /// 主要用于内存监控等需要每帧重新捕获快照的对话框，在节流刷新前调用，
    /// 确保 `render_iced_ui` 不因 `ui_dirty == false` 而跳过 `UserInterface::build`。
    pub fn mark_dirty(&mut self) {
        self.ui_dirty = true;
    }

    /// 收集所有组件的内存占用快照（Root + RenderCache）
    pub fn memory_breakdown(&self) -> root::MemoryBreakdown {
        let mut breakdown = self.root.memory_breakdown();

        // 从 RenderCache 获取主音符三缓冲容量
        let (writer_cap, writer_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(0);
        let (ready_cap, ready_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(1);
        let (reading_cap, reading_len) = self
            .render_ctx
            .render_cache
            .note_instances_buffer
            .buffer_info(2);
        // 将三缓冲容量写入 breakdown 的附加字段
        breakdown.note_instances_writer_cap = writer_cap;
        breakdown.note_instances_writer_len = writer_len;
        breakdown.note_instances_ready_cap = ready_cap;
        breakdown.note_instances_ready_len = ready_len;
        breakdown.note_instances_reading_cap = reading_cap;
        breakdown.note_instances_reading_len = reading_len;
        breakdown.note_instance_size = std::mem::size_of::<lumino_gfx::NoteInstance>() as usize;

        breakdown
    }

    /// 路由消息：先检查直接处理，未处理则通过路由器分发
    pub(crate) fn route_message(&mut self, msg: message::Message) {
        self.root.poll_midi_input();
        if let message::Message::Batch(messages) = msg {
            for m in messages {
                self.route_message(m);
            }
            return;
        }
        if !self.root.try_handle_direct(&msg) {
            self.message_router.route(&mut self.root, msg);
        }
    }
}

/// 在后台线程预热对话框共享的 iced Engine。
///
/// 主窗口创建后调用，可在用户打开第一个对话框前完成 pipeline 创建，
/// 避免对话框初始化阻塞 900ms+。若线程未完成，首个对话框会等待该线程结束
/// 或使用 `get_or_init` 自己创建。
pub fn prewarm_dialog_shared_engine(gfx: &lumino_gfx::Context) {
    let adapter = gfx.adapter.clone();
    let device = gfx.device.clone();
    let queue = gfx.queue.clone();
    let format = gfx.format;

    std::thread::spawn(move || {
        let _ = render_ctx::SHARED_ENGINE.get_or_init(|| {
            Engine::new(
                &adapter,
                device,
                queue,
                format,
                None,
                iced_wgpu::graphics::Shell::headless(),
            )
        });
    });
}

/// 洋葱皮音轨调色板（按音轨索引循环取色）
///
/// 从当前调色板的第一个颜色开始取色。
fn onion_track_color(track_idx: usize) -> [u8; 4] {
    lumino_core::palette::onion_track_color(track_idx)
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
