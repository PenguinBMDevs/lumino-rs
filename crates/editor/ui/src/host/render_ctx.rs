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
    /// 本帧是否携带「必须在 present 前立即可见」的内容变化。
    ///
    /// 生产者（音符编辑增量 / 洋葱皮重传 / 预览变化 / 视口尺寸变化）置位，
    /// `build_render_params` 消费并写入 `RenderParams::content_dirty`。
    /// 纯视口变化帧保持 `false`，UI 线程据此跳过 `wait_for_frame`。
    pub render_content_dirty: bool,
    /// 上一次实际提交渲染参数时的物理视口尺寸（`None` = 尚未提交过）。
    ///
    /// 尺寸变化会触发渲染线程重建离屏纹理：此时必须等待本帧渲染完成，
    /// 否则可能拷到刚创建、尚未渲染的空纹理。
    pub last_sent_viewport: Option<(u32, u32)>,
    /// 上一次发送给渲染线程的预览实例。
    ///
    /// 用途有二：① 判定预览是否真的变化——内容不变的 hover 预览若每帧把
    /// `render_content_dirty` 置位，滚动/缩放时会永远走等待路径，在途帧闸门失效；
    /// ② 未变化时不再重复下发 `PreviewInstances`，顺带消除渲染侧每帧重建
    /// cull/render bind group 的固定开销。
    pub last_preview_instances: Vec<lumino_gfx::NoteInstance>,
    /// 上一次**实际提交**渲染参数时的视口指纹（`None` = 尚未提交过）。
    ///
    /// 只在真正 `send_params` 时更新：被闸门跳过的帧不更新，否则会把未提交的
    /// 视口状态误记为已提交，导致后续帧误判「视口稳定」。
    pub last_view_key: Option<RenderViewKey>,
    /// 已提交但尚未同步呈现（未经 `wait_for_frame`）的帧号。
    ///
    /// 非内容脏帧提交后画面落后一帧；视口稳定后由一次 `Flush` 同步刷上屏。
    /// 没有这个收尾，「单次 Ctrl+滚轮缩放 / 拖拽最后一帧」这类只产生一帧的
    /// 视口变化将永远不会被呈现（最后一次变化停在离屏纹理里）。
    pub pending_present_flush: Option<u64>,
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
            render_content_dirty: false,
            last_sent_viewport: None,
            last_preview_instances: Vec::new(),
            last_view_key: None,
            pending_present_flush: None,
            device: wgpu.device.clone(),
            queue: wgpu.queue.clone(),
            format: wgpu.format,
            keyboard_renderer: None,
        }
    }
}

/// 两组预览实例是否完全相同（逐字段比较，避免额外依赖与整块字节拷贝）。
pub(crate) fn same_preview_instances(
    a: &[lumino_gfx::NoteInstance],
    b: &[lumino_gfx::NoteInstance],
) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.start_length == y.start_length
                && x.key_color == y.key_color
                && x.border_width == y.border_width
        })
}

/// 影响离屏画面内容的「视口参数指纹」。
///
/// 用途：判定本帧相对上一次**实际提交**的参数，是否发生了变化。指纹不变 ⇒
/// 离屏渲染结果与已提交帧等价，只需「同步呈现已提交帧」即可，无需再堆叠一次
/// 离屏渲染（`Flush` 路径）。
///
/// 覆盖所有参与 GPU 绘制输入的标量与矩形参数；派生实例向量（网格线 / 标尺刻度 /
/// 走带覆盖层 / CC 柱）本身不参与比较——它们是上述参数的纯函数，文档变化则由
/// `RenderParams::content_dirty` 走等待路径覆盖。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct RenderViewKey {
    scroll: (f32, f32),
    zoom: (f32, f32),
    logical_size: (f32, f32),
    viewport_size: (u32, u32),
    scale_factor: f32,
    canvas_offset: (f32, f32),
    canvas_size: (f32, f32),
    keyboard_width: f32,
    ruler_height: f32,
    max_key_index: f32,
    ppq: f32,
    is_vertical_roll: bool,
    is_arrangement_mode: bool,
    skip_scene_render: bool,
    velocity_panel_rect: Option<(f32, f32, f32, f32)>,
}

impl RenderViewKey {
    /// 从渲染参数提取指纹。
    pub(crate) fn from_params(params: &lumino_gfx::RenderParams) -> Self {
        Self {
            scroll: params.scroll,
            zoom: params.zoom,
            logical_size: params.logical_size,
            viewport_size: params.viewport_size,
            scale_factor: params.scale_factor,
            canvas_offset: params.canvas_offset,
            canvas_size: params.canvas_size,
            keyboard_width: params.keyboard_width,
            ruler_height: params.ruler_height,
            max_key_index: params.max_key_index,
            ppq: params.ppq,
            is_vertical_roll: params.is_vertical_roll,
            is_arrangement_mode: params.is_arrangement_mode,
            skip_scene_render: params.skip_scene_render,
            velocity_panel_rect: params.velocity_panel_rect,
        }
    }
}

impl RenderContext {
    /// 标记本帧携带「必须在 present 前立即可见」的内容变化。
    ///
    /// 调用点仅限**新产生数据**的路径：音符编辑增量、洋葱皮重传、预览实例变化、
    /// 视口尺寸变化（离屏纹理重建）。随视口每帧重算的派生实例（网格/标尺/
    /// 走带覆盖层/CC 柱）**不**算内容变化——它们落后一帧无感知。
    pub(crate) fn mark_render_content_dirty(&mut self) {
        self.render_content_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_gfx::{NoteInstance, RenderParams};

    fn inst(tick: f32) -> NoteInstance {
        NoteInstance::new(tick, 60, 10.0, [1.0, 0.0, 0.0, 1.0], 1)
    }

    /// 视口指纹必须对「绘制输入标量」敏感，对「每帧重算的派生实例向量」不敏感。
    ///
    /// 若派生实例混入指纹，静止画面的每一次 UI 重绘都会判定「视口变了」，
    /// 收尾帧（Flush）逻辑与免等待策略一起失效。
    #[test]
    fn test_view_key_ignores_derived_instance_vectors() {
        let base = RenderParams::default();
        let mut with_notes = base.clone();
        with_notes.note_instances.push(inst(0.0));
        with_notes.note_instances.push(inst(480.0));

        assert_eq!(
            RenderViewKey::from_params(&base),
            RenderViewKey::from_params(&with_notes),
            "派生实例向量不应参与视口指纹"
        );
    }

    /// 滚动 / 缩放 / 画布 / 力度面板矩形变化必须被指纹捕获——否则视口变化帧
    /// 会被误判为「视口稳定」，既不做收尾也不再提交新渲染。
    #[test]
    fn test_view_key_tracks_drawing_inputs() {
        let base = RenderParams::default();

        let mut scrolled = base.clone();
        scrolled.scroll = (base.scroll.0 + 1.0, base.scroll.1);
        assert_ne!(
            RenderViewKey::from_params(&base),
            RenderViewKey::from_params(&scrolled),
            "滚动变化必须改变视口指纹"
        );

        let mut zoomed = base.clone();
        zoomed.zoom = (base.zoom.0, base.zoom.1 * 2.0);
        assert_ne!(
            RenderViewKey::from_params(&base),
            RenderViewKey::from_params(&zoomed),
            "缩放变化必须改变视口指纹"
        );

        let mut panel = base.clone();
        panel.velocity_panel_rect = Some((0.0, 0.0, 100.0, 80.0));
        assert_ne!(
            RenderViewKey::from_params(&base),
            RenderViewKey::from_params(&panel),
            "力度面板矩形变化必须改变视口指纹（非内容脏帧只能靠指纹触发收尾）"
        );

        let mut resized = base.clone();
        resized.viewport_size = (base.viewport_size.0 + 16, base.viewport_size.1);
        assert_ne!(
            RenderViewKey::from_params(&base),
            RenderViewKey::from_params(&resized),
            "视口尺寸变化必须改变视口指纹"
        );
    }

    /// 预览实例变化检测：内容相同视为未变化（避免滚动时每帧置脏），
    /// 任一字段不同都必须判定为变化（否则预览会停在旧内容）。
    #[test]
    fn test_same_preview_instances() {
        assert!(same_preview_instances(&[], &[]));
        assert!(!same_preview_instances(&[], &[inst(0.0)]));
        assert!(same_preview_instances(&[inst(0.0)], &[inst(0.0)]));
        assert!(!same_preview_instances(&[inst(0.0)], &[inst(1.0)]));

        // border_width 参与比较（预览哨兵值 / 轨道深度编码都在其中）
        let mut other_border = inst(0.0);
        other_border.border_width = 2;
        assert!(!same_preview_instances(&[inst(0.0)], &[other_border]));
    }
}
