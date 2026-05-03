//! 渲染上下文 —— 从 Host 拆出的渲染相关字段
//!
//! 管理 iced 渲染器、wgpu 音符/网格渲染器、GPU 资源以及独立渲染线程。

use iced_core::{Font, Pixels, mouse};
use iced_wgpu::{Engine, Renderer, graphics::Viewport};
use iced_wgpu::wgpu;
use iced_winit::runtime::user_interface::Cache;
use lumino_gfx::{GridRenderer, NoteRenderer};

use super::RenderCache;

/// 渲染上下文，持有所有渲染所需的 GPU 资源和渲染器实例。
pub(crate) struct RenderContext {
    /// iced 渲染器
    pub renderer: Renderer,
    /// UI 缓存树
    pub cache: Cache,
    /// 视口信息
    pub viewport: Viewport,
    /// 音符渲染器
    pub note_renderer: NoteRenderer,
    /// 网格渲染器
    pub grid_renderer: GridRenderer,
    /// 渲染缓存
    pub render_cache: RenderCache,
    /// 上次编辑状态
    pub last_edit_state: crate::editor::EditState,
    /// 上次光标位置
    pub last_cursor_position: Option<iced_core::Point>,
    /// 上次渲染光标状态
    pub last_render_cursor: mouse::Cursor,
    /// 渲染线程
    pub wgpu_render_thread: Option<crate::WgpuRenderThread>,
    /// 渲染线程通信
    pub note_events_tx: Option<std::sync::mpsc::Sender<lumino_gfx::NoteEvent>>,
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
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        adapter: &wgpu::Adapter,
        viewport: Viewport,
        note_renderer: NoteRenderer,
        grid_renderer: GridRenderer,
        font: Font,
    ) -> Self {
        let engine = Engine::new(
            adapter,
            device.clone(),
            queue.clone(),
            format,
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
            last_render_cursor: mouse::Cursor::Unavailable,
            wgpu_render_thread: None,
            note_events_tx: None,
            use_separate_render_thread: false,
            has_rendered_ui: false,
            device,
            queue,
            format,
        }
    }
}
