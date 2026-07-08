//! 渲染上下文 —— 从 Host 拆出的渲染相关字段
//!
//! 管理 iced 渲染器、wgpu 音符/网格渲染器、GPU 资源以及独立渲染线程。

use iced_core::{Font, Pixels};
use iced_wgpu::wgpu;
use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use iced_winit::runtime::user_interface::Cache;
use lumino_gfx::{GridRenderer, NoteRenderer};

use super::RenderCache;

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
    /// 网格渲染器（仅主窗口需要）
    pub grid_renderer: Option<GridRenderer>,
    /// 渲染缓存
    pub render_cache: RenderCache,
    /// 上次编辑状态
    pub last_edit_state: crate::editor::EditState,
    /// 上次光标位置
    pub last_cursor_position: Option<iced_core::Point>,
    /// 渲染线程
    pub wgpu_render_thread: Option<crate::WgpuRenderThread>,
    /// 分离渲染架构标识
    pub use_separate_render_thread: bool,
    /// 首次渲染标识
    pub has_rendered_ui: bool,
    // WGPU 资源
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
}

impl RenderContext {
    /// 创建渲染上下文
    ///
    /// `note_renderer` 与 `grid_renderer` 为 `None` 时，表示该窗口仅渲染 iced UI，
    /// 不进入音符/网格管线（用于 dialog、progress 等轻量窗口）。
    pub fn new(
        wgpu: &WgpuResources,
        viewport: Viewport,
        note_renderer: Option<NoteRenderer>,
        grid_renderer: Option<GridRenderer>,
        font: Font,
    ) -> Self {
        let engine = Engine::new(
            &wgpu.adapter,
            wgpu.device.clone(),
            wgpu.queue.clone(),
            wgpu.format,
            None,
            iced_wgpu::graphics::Shell::headless(),
        );
        let renderer = Renderer::new(engine, font, Pixels::from(16));

        Self {
            renderer,
            cache: Cache::new(),
            viewport,
            note_renderer,
            grid_renderer,
            render_cache: RenderCache::new(),
            last_edit_state: crate::editor::EditState::default(),
            last_cursor_position: None,
            wgpu_render_thread: None,
            use_separate_render_thread: false,
            has_rendered_ui: false,
            device: wgpu.device.clone(),
            queue: wgpu.queue.clone(),
            format: wgpu.format,
        }
    }
}
