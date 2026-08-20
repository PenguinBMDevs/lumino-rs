//! 渲染上下文 —— 从 Host 拆出的渲染相关字段
//!
//! 管理 iced 渲染器、wgpu 音符/网格渲染器、GPU 资源以及独立渲染线程。

use std::sync::{Arc, OnceLock};

use iced_core::{Font, Pixels};
use iced_wgpu::wgpu;
use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use iced_winit::runtime::user_interface::Cache;
use lumino_gfx::NoteRenderer;

use super::RenderCache;

/// 所有对话框共享的 iced Engine。
///
/// 主窗口单独维护自己的 Engine（带 WindowNotifier），对话框使用 headless shell
/// 的共享 Engine，避免每个对话框重复创建 pipeline 产生 900ms+ 阻塞。
/// Engine 内部持有 device/queue/format/pipeline 等，Clone 成本远低于重新创建。
pub(crate) static SHARED_ENGINE: OnceLock<Engine> = OnceLock::new();

/// 通知器：当后台图像上传完成时，请求窗口重绘
struct WindowNotifier(Arc<iced_winit::winit::window::Window>);

impl iced_wgpu::graphics::shell::Notifier for WindowNotifier {
    fn request_redraw(&self) {
        self.0.request_redraw();
    }

    fn invalidate_layout(&self) {
        // 布局失效也触发重绘，确保 image atlas 上传后能刷新
        self.0.request_redraw();
    }
}

/// WGPU 设备资源集合（减少 RenderContext::new 参数数量）
pub(crate) struct WgpuResources {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub adapter: wgpu::Adapter,
}

/// 渲染上下文，持有所有渲染所需的 GPU 资源和渲染器实例。
pub(crate) struct RenderContext {
    /// iced 渲染器
    pub renderer: Renderer,
    /// UI 缓存树
    pub cache: Cache,
    /// 视口信息
    pub viewport: Viewport,
    /// 音符渲染器（仅主窗口需要）
    pub note_renderer: Option<NoteRenderer>,
    /// 洋葱皮状态缓存（跟踪 track_notes_gen + 音轨开关变化）
    pub onion_skin_state: crate::host::render::onion_skin::OnionSkinState,
    /// 渲染缓存
    pub render_cache: RenderCache,
    /// 上次光标位置
    pub last_cursor_position: Option<iced_core::Point>,
    /// 渲染线程
    pub wgpu_render_thread: Option<crate::WgpuRenderThread>,
    /// 首次渲染标识
    pub has_rendered_ui: bool,
    // WGPU 资源
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    /// 钢琴瀑布流面板键盘离屏渲染器（按需懒创建，跨帧复用管线）
    pub keyboard_renderer:
        Option<crate::right_sidebar::piano_waterfall::keyboard_renderer::KeyboardRenderer>,
}

impl RenderContext {
    /// 创建渲染上下文
    ///
    /// `note_renderer` 为 `None` 时，表示该窗口仅渲染 iced UI，
    /// 不进入音符/网格管线（用于 dialog、progress 等轻量窗口）。
    ///
    /// `window` 用于创建通知器：当 iced_wgpu 后台完成图像上传后，
    /// 通知器会调用 `window.request_redraw()` 触发窗口重绘，否则
    /// 大尺寸预览图像（>2MB）的异步上传完成后窗口不会刷新，导致预览空白。
    ///
    /// `use_shared_engine` 为 `true` 时，复用对话框共享的 Engine（headless shell），
    /// 避免重复创建 pipeline。仅对 dialog 构造函数开启；主窗口需要独立 Notifier，
    /// 保持 `false`。
    pub fn new(
        wgpu: &WgpuResources,
        viewport: Viewport,
        note_renderer: Option<NoteRenderer>,
        font: Font,
        window: &Arc<iced_winit::winit::window::Window>,
        use_shared_engine: bool,
    ) -> Self {
        puffin::profile_function!();
        let engine = if use_shared_engine {
            // 对话框：复用全局共享 Engine，内部 pipeline 只需创建一次。
            SHARED_ENGINE
                .get_or_init(|| {
                    puffin::profile_scope!("shared_engine_create");
                    Engine::new(
                        &wgpu.adapter,
                        wgpu.device.clone(),
                        wgpu.queue.clone(),
                        wgpu.format,
                        None,
                        iced_wgpu::graphics::Shell::headless(),
                    )
                })
                .clone()
        } else {
            // 主窗口 / progress 窗口：使用独立 Engine + 当前窗口 Notifier。
            let shell = iced_wgpu::graphics::Shell::new(WindowNotifier(Arc::clone(window)));
            Engine::new(
                &wgpu.adapter,
                wgpu.device.clone(),
                wgpu.queue.clone(),
                wgpu.format,
                None,
                shell,
            )
        };

        let renderer = Renderer::new(engine, font, Pixels::from(16));

        Self {
            renderer,
            cache: Cache::new(),
            viewport,
            note_renderer,
            onion_skin_state: Default::default(),
            render_cache: RenderCache::new(),
            last_cursor_position: None,
            wgpu_render_thread: None,
            has_rendered_ui: false,
            device: wgpu.device.clone(),
            queue: wgpu.queue.clone(),
            format: wgpu.format,
            keyboard_renderer: None,
        }
    }
}
