//! winit `ApplicationHandler`：窗口 / surface / iced Engine 的生命周期与每帧绘制。
//!
//! 与 `android-iced-example` 同构：自己驱动 winit 事件循环，把 `WindowEvent` 交给
//! `iced_winit::conversion` 转成 iced 事件，再用 `UserInterface` 手工更新/绘制，
//! 最后由 `iced_wgpu::Renderer::present` 直接画到 swapchain 视图。

use std::sync::Arc;
use std::time::Instant;

use iced_wgpu::graphics::{Shell, Viewport};
use iced_wgpu::{Engine, Renderer, wgpu};
use iced_winit::core::{Color, Event, Pixels, Size, Theme, mouse, renderer, window};
use iced_winit::runtime::user_interface::{self, UserInterface};
use iced_winit::{conversion, winit};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::font;
use crate::ui::{Message, Probe};

/// 清屏色（同时作为 UI 背景）。
const BACKGROUND: Color = Color::from_rgb(0.08, 0.09, 0.11);
/// 基准字号（px）。
const BASE_FONT_SIZE: u16 = 16;

/// 探针应用状态。
pub struct App {
    graphics: Option<Graphics>,
    ui: Probe,
    cache: user_interface::Cache,
    cursor: mouse::Cursor,
    events: Vec<Event>,
    modifiers: ModifiersState,
    resized: bool,
}

/// 图形栈（窗口 + surface + 渲染器），`suspend` 后整体丢弃重建。
struct Graphics {
    window: Arc<Window>,
    device: wgpu::Device,
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    renderer: Renderer,
    viewport: Viewport,
    clipboard: iced_winit::Clipboard,
}

impl App {
    /// 新建应用（字体在进事件循环前注册，避免首帧无字形）。
    pub fn new() -> Self {
        // 字体必须早于 `Renderer::new`：渲染器构造时从全局 font_system 取默认字体。
        let _font = font::register();

        Self {
            graphics: None,
            ui: Probe::new(),
            cache: user_interface::Cache::new(),
            cursor: mouse::Cursor::Unavailable,
            events: Vec::new(),
            modifiers: ModifiersState::default(),
            resized: false,
        }
    }

    /// 绘制一帧，返回本轮产生的 UI 消息。
    fn paint(&mut self, graphics: &mut Graphics) {
        let frame = match graphics.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(error) => {
                log::warn!("获取当前帧失败（下一帧重试）: {error}");
                graphics.window.request_redraw();
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut interface = UserInterface::build(
            self.ui.view(),
            graphics.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut graphics.renderer,
        );

        self.events.push(Event::Window(
            window::Event::RedrawRequested(Instant::now()),
        ));
        let mut messages = Vec::new();
        let _ = interface.update(
            &self.events,
            self.cursor,
            &mut graphics.renderer,
            &mut graphics.clipboard,
            &mut messages,
        );
        self.events.clear();

        let theme = Theme::Dark;
        interface.draw(
            &mut graphics.renderer,
            &theme,
            &renderer::Style {
                text_color: theme.palette().text,
            },
            self.cursor,
        );
        self.cache = interface.into_cache();

        graphics
            .renderer
            .present(Some(BACKGROUND), graphics.format, &view, &graphics.viewport);
        frame.present();

        self.apply(messages);
    }

    /// 消费 UI 消息；`Action` 型消息（无）直接丢弃。
    fn apply(&mut self, messages: Vec<Message>) {
        if messages.is_empty() {
            return;
        }
        for message in messages {
            self.ui.update(message);
        }
    }

    /// 处理一次窗口事件。
    fn handle(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        let Some(graphics) = self.graphics.take() else {
            return;
        };

        let mut needs_redraw = false;

        match &event {
            WindowEvent::CloseRequested => {
                self.graphics = Some(graphics);
                event_loop.exit();
                return;
            }
            WindowEvent::RedrawRequested => {
                self.graphics = Some(graphics);
                self.redraw();
                return;
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.resized = true;
                    needs_redraw = true;
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.resized = true;
                needs_redraw = true;
            }
            WindowEvent::Touch(touch) => {
                self.cursor = mouse::Cursor::Available(conversion::cursor_position(
                    touch.location,
                    graphics.viewport.scale_factor(),
                ));
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = mouse::Cursor::Available(conversion::cursor_position(
                    *position,
                    graphics.viewport.scale_factor(),
                ));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                log::debug!("按键: {:?}", event.logical_key);
            }
            _ => {}
        }

        if let Some(event) =
            conversion::window_event(event, graphics.viewport.scale_factor(), self.modifiers)
        {
            self.events.push(event);
            needs_redraw = true;
        }

        self.graphics = Some(graphics);

        if needs_redraw {
            self.redraw();
        }
    }

    /// 重绘入口：必要时先按新尺寸重配 surface。
    fn redraw(&mut self) {
        let Some(mut graphics) = self.graphics.take() else {
            return;
        };

        if self.resized {
            graphics.resize();
            self.resized = false;
        }

        self.paint(&mut graphics);
        self.graphics = Some(graphics);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("resumed：重建窗口 / surface / 渲染器");

        // android 每次回前台都换 native window，旧 surface 必须整体丢弃；
        // 正式版的「保留交互态」重建（不重建 Cache 树）属于 L2 的工作。
        self.graphics = None;
        self.events.clear();

        event_loop.set_control_flow(ControlFlow::Wait);

        match Graphics::new(event_loop) {
            Ok(graphics) => {
                graphics.window.request_redraw();
                self.graphics = Some(graphics);
            }
            Err(error) => {
                log::error!("初始化图形栈失败: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.handle(event_loop, event);
    }
}

impl Graphics {
    /// 建立窗口 → surface → adapter/device → iced Engine 全链路。
    fn new(event_loop: &ActiveEventLoop) -> Result<Self, String> {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .map_err(|error| format!("创建窗口失败: {error}"))?,
        );

        // android：set_ime_allowed(true) 即触发 show_soft_input。
        window.set_ime_allowed(true);

        let size = window.inner_size();
        let viewport = Viewport::with_physical_size(
            Size::new(size.width.max(1), size.height.max(1)),
            window.scale_factor() as f32,
        );

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        let surface = instance
            .create_surface(Arc::clone(&window))
            .map_err(|error| format!("创建 surface 失败: {error}"))?;

        let (format, adapter, device, queue) =
            futures::executor::block_on(create_device(&instance, &surface))?;

        surface.configure(
            &device,
            &surface_config(format, size.width.max(1), size.height.max(1)),
        );

        // `Shell::headless()`：探针不需要 iced 反向通知重绘（我们自己在事件后重绘）。
        let engine = Engine::new(
            &adapter,
            device.clone(),
            queue,
            format,
            None,
            Shell::headless(),
        );
        let renderer = Renderer::new(
            engine,
            crate::font::register(),
            Pixels::from(f32::from(BASE_FONT_SIZE)),
        );
        let clipboard = iced_winit::Clipboard::connect(Arc::clone(&window));

        log::info!(
            "图形栈就绪: surface={}x{} scale={} format={format:?} adapter={:?}",
            size.width,
            size.height,
            window.scale_factor(),
            adapter.get_info().name
        );

        Ok(Self {
            window,
            device,
            surface,
            format,
            renderer,
            viewport,
            clipboard,
        })
    }

    /// 按当前窗口尺寸重配 surface 与 viewport。
    fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.viewport = Viewport::with_physical_size(
            Size::new(size.width, size.height),
            self.window.scale_factor() as f32,
        );
        self.surface.configure(
            &self.device,
            &surface_config(self.format, size.width, size.height),
        );
    }
}

/// surface 配置（configure / 重配共用，避免两处漂移）。
fn surface_config(
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}

/// 选 adapter 与 device；优先 sRGB format。
async fn create_device(
    instance: &wgpu::Instance,
    surface: &wgpu::Surface<'static>,
) -> Result<
    (
        wgpu::TextureFormat,
        wgpu::Adapter,
        wgpu::Device,
        wgpu::Queue,
    ),
    String,
> {
    let adapter = wgpu::util::initialize_adapter_from_env_or_default(instance, Some(surface))
        .await
        .map_err(|error| format!("获取 adapter 失败: {error}"))?;

    let capabilities = surface.get_capabilities(&adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| "surface 无可用纹理格式".to_string())?;

    let limits = adapter.limits();
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("lumino-probe"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        })
        .await
        .map_err(|error| format!("请求 device 失败: {error}"))?;

    Ok((format, adapter, device, queue))
}
