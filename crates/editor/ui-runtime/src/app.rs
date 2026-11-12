//! 最小原生运行时：winit 事件循环 + wgpu 27 Surface + egui 0.37 + 自有 bridge。

use std::sync::Arc;
use std::time::Instant;

use egui::{Event as EguiEvent, PointerButton as EguiButton, Pos2, RawInput, Rect, Vec2};
use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, TitlebarModel, ToolbarModel};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// 启动原生 egui 运行时探针。
pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = NativeApp::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// 原生运行时应用状态。
#[derive(Default)]
pub struct NativeApp {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    ctx: egui::Context,
    icons: lumino_ui_egui::icons::IconCache,
    events: Vec<EguiEvent>,
    pointer: Option<Pos2>,
    modifiers: ModifiersState,
    clicks: u32,
    start: Option<Instant>,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

impl Gpu {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|error| format!("创建 Surface 失败: {error}"))?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("请求 GPU 适配器失败: {error:?}"))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| format!("创建 wgpu 设备失败: {error:?}"))?;

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| "Surface 不支持默认配置".to_owned())?;
        surface.configure(&device, &config);
        let painter = Painter::new(&device, config.format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            painter,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn present(&mut self, ctx: &egui::Context, mut output: egui::FullOutput) {
        let Ok(frame) = self.surface.get_current_texture() else {
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lumino-runtime-encoder"),
            });

        // 先清屏（Painter 采用 LoadOp::Load 叠加绘制）。
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lumino-runtime-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.062,
                            g: 0.066,
                            b: 0.078,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        let primitives =
            ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        self.painter
            .update_textures(&self.device, &self.queue, &mut output.textures_delta);
        self.painter.render(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            Screen {
                size_px: [self.config.width, self.config.height],
                pixels_per_point: output.pixels_per_point,
            },
            &primitives,
        );

        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Lumino · egui native runtime")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                tracing::error!("创建窗口失败: {error}");
                event_loop.exit();
                return;
            }
        };
        match Gpu::new(window.clone()) {
            Ok(gpu) => self.gpu = Some(gpu),
            Err(error) => {
                tracing::error!("初始化 wgpu 失败: {error}");
                event_loop.exit();
                return;
            }
        }
        self.ctx.set_visuals(egui::Visuals::dark());
        self.ctx.set_pixels_per_point(window.scale_factor() as f32);
        self.start = Some(Instant::now());
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let scale = window.scale_factor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.ctx.set_pixels_per_point(window.scale_factor() as f32);
                window.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let logical = position.to_logical::<f32>(scale);
                let pos = Pos2::new(logical.x, logical.y);
                self.pointer = Some(pos);
                self.events.push(EguiEvent::PointerMoved(pos));
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                self.events.push(EguiEvent::PointerGone);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(pos) = self.pointer {
                    self.events.push(EguiEvent::PointerButton {
                        pos,
                        button: pointer_button(button),
                        pressed: state == ElementState::Pressed,
                        modifiers: modifiers(self.modifiers),
                    });
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (unit, delta) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        (egui::MouseWheelUnit::Line, Vec2::new(x, y))
                    }
                    MouseScrollDelta::PixelDelta(delta) => {
                        let logical = delta.to_logical::<f32>(scale);
                        (egui::MouseWheelUnit::Point, Vec2::new(logical.x, logical.y))
                    }
                };
                self.events.push(EguiEvent::MouseWheel {
                    unit,
                    delta,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: modifiers(self.modifiers),
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let Some(key) = egui_key(&event.logical_key)
                {
                    self.events.push(EguiEvent::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: modifiers(self.modifiers),
                    });
                }
                if let Some(text) = &event.text
                    && !text.is_empty()
                    && !self.modifiers.control_key()
                    && !self.modifiers.super_key()
                {
                    self.events.push(EguiEvent::Text(text.to_string()));
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // 探针保持 60Hz 重绘（egui 悬停/按钮反馈需要连续帧）。
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + std::time::Duration::from_millis(16),
        ));
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl NativeApp {
    fn render(&mut self) {
        let (Some(window), Some(gpu)) = (self.window.as_ref(), self.gpu.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let ppp = window.scale_factor() as f32;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(size.width as f32 / ppp, size.height as f32 / ppp),
            )),
            events: std::mem::take(&mut self.events),
            time: self.start.map(|start| start.elapsed().as_secs_f64()),
            ..Default::default()
        };

        let clicks = &mut self.clicks;
        let mut icons = std::mem::take(&mut self.icons);
        let output = self.ctx.run_ui(raw, |ui| {
            // 直接跑生产 egui 外壳（标题栏 / 工具栏 / 左右栏 / 状态栏）。
            let model = demo_model(*clicks);
            chrome::show_bands(ui, &model, &mut icons);

            // 探针控件：验证输入仍能到达 egui。
            egui::Area::new(egui::Id::unique("lumino-runtime-probe"))
                .fixed_pos(egui::pos2(24.0, 96.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("native runtime probe");
                        if ui.button("点我 +1").clicked() {
                            *clicks += 1;
                        }
                        ui.label(format!("clicks {clicks}"));
                    });
                });
        });
        self.icons = icons;

        gpu.present(&self.ctx, output);
    }
}

/// 探针用的最小外壳模型：标题栏 + 默认工具栏 + 状态栏。
fn demo_model(clicks: u32) -> ChromeModel {
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
            menu: None,
            menu_only: false,
        }),
        toolbar: Some(ToolbarModel::default()),
        statusbar: StatusbarModel {
            left: format!("Lumino egui native runtime · clicks {clicks}"),
            left_accent: true,
            cpu_percent: None,
            memory_mb: 0.0,
            fps: None,
            right: String::new(),
            gpu: None,
        },
        ..ChromeModel::default()
    }
}

fn pointer_button(button: MouseButton) -> EguiButton {
    match button {
        MouseButton::Left => EguiButton::Primary,
        MouseButton::Right => EguiButton::Secondary,
        MouseButton::Middle => EguiButton::Middle,
        _ => EguiButton::Extra1,
    }
}

fn modifiers(state: ModifiersState) -> egui::Modifiers {
    egui::Modifiers {
        alt: state.alt_key(),
        ctrl: state.control_key(),
        shift: state.shift_key(),
        mac_cmd: state.super_key(),
        command: state.control_key() || state.super_key(),
    }
}

fn egui_key(key: &Key) -> Option<egui::Key> {
    Some(match key {
        Key::Named(NamedKey::Escape) => egui::Key::Escape,
        Key::Named(NamedKey::Enter) => egui::Key::Enter,
        Key::Named(NamedKey::Space) => egui::Key::Space,
        Key::Named(NamedKey::Tab) => egui::Key::Tab,
        Key::Named(NamedKey::Backspace) => egui::Key::Backspace,
        Key::Named(NamedKey::Delete) => egui::Key::Delete,
        Key::Named(NamedKey::ArrowLeft) => egui::Key::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => egui::Key::ArrowRight,
        Key::Named(NamedKey::ArrowUp) => egui::Key::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => egui::Key::ArrowDown,
        _ => return None,
    })
}
