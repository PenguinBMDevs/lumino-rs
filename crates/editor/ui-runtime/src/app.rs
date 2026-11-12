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
    root: Option<lumino_ui::Root>,
    canvas: Option<GfxCanvas>,
    events: Vec<EguiEvent>,
    pointer: Option<Pos2>,
    modifiers: ModifiersState,
    clicks: u32,
    /// 是否正在用 egui 指针驱动编辑器（按下 → Moved → Released）。
    editor_dragging: bool,
    start: Option<Instant>,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

/// 编辑器画布：`lumino-gfx` 分离渲染线程 + 离屏纹理（供 egui 作为底层采样）。
struct GfxCanvas {
    thread: lumino_gfx::WgpuRenderThread,
    /// 音符实例双缓冲；空画布也保留句柄，后续接入编辑器数据即可复用。
    _buffer: Arc<lumino_gfx::SwappableBuffer<lumino_gfx::NoteInstance>>,
}

impl GfxCanvas {
    fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        let (note_tx, note_rx) = std::sync::mpsc::channel();
        let buffer = Arc::new(lumino_gfx::SwappableBuffer::new(0));
        let thread = lumino_gfx::WgpuRenderThread::spawn(
            device.clone(),
            queue.clone(),
            format,
            lumino_gfx::GpuEpoch::INITIAL,
            note_tx,
            note_rx,
            buffer.clone(),
        )
        .map_err(|error| format!("启动 gfx 渲染线程失败: {error}"))?;
        Ok(Self {
            thread,
            _buffer: buffer,
        })
    }

    /// 提交一帧并等待离屏纹理就绪，返回可采样的纹理视图。
    fn render(
        &self,
        width: u32,
        height: u32,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> Option<Arc<wgpu::TextureView>> {
        let params = lumino_gfx::RenderParams {
            viewport_size: (width.max(1), height.max(1)),
            logical_size: (logical_width, logical_height),
            scale_factor,
            canvas_size: (logical_width, logical_height),
            // 与 egui 深色底色区分，便于真机确认底层纹理确实来自 gfx。
            background_color: [0.07, 0.10, 0.16, 1.0],
            color_bg: [0.07, 0.10, 0.16, 1.0],
            content_dirty: true,
            ..Default::default()
        };
        let frame_id = self.thread.send_params(params);
        self.thread.wait_for_frame(frame_id);
        self.thread.latest_texture_view()
    }
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
            Ok(gpu) => match GfxCanvas::new(&gpu.device, &gpu.queue, gpu.config.format) {
                Ok(canvas) => {
                    self.canvas = Some(canvas);
                    self.gpu = Some(gpu);
                }
                Err(error) => {
                    tracing::error!("初始化 gfx 画布失败: {error}");
                    event_loop.exit();
                    return;
                }
            },
            Err(error) => {
                tracing::error!("初始化 wgpu 失败: {error}");
                event_loop.exit();
                return;
            }
        }
        self.ctx.set_visuals(egui::Visuals::dark());
        self.ctx.set_pixels_per_point(window.scale_factor() as f32);
        let mut root = lumino_ui::Root::new(&lumino_core::storage::config::UiConfig::default());
        // 空工程 → 建两条音轨（0 指挥 / 1 普通），当前轨设为 1，便于铅笔落音符。
        let mut doc = lumino_midi_loader::MidiDocument::empty_with_tracks(2, 480);
        doc.tempo_changes = vec![(0, 120.0)];
        doc.time_signatures = vec![(0, 4, 4)];
        root.set_midi_document(doc);
        root.editor.editor_state.view.ppq = 480;
        root.editor.editor_state.data.current_track = 1;
        // 走生产工具栏事件：同时更新 toolbar 与 editor 的工具状态。
        root.update(lumino_ui_core::toolbar_event::Event::tool_selected(
            lumino_core::Tool::Pencil,
        ));
        self.root = Some(root);
        self.sync_editor_canvas(&window);
        self.start = Some(Instant::now());
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let scale = window.scale_factor();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.resize(size.width, size.height);
                }
                self.sync_editor_canvas(&window);
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.ctx.set_pixels_per_point(window.scale_factor() as f32);
                self.sync_editor_canvas(&window);
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
                if self.editor_dragging
                    && let Some(local) = self.editor_local(pos)
                {
                    self.send_editor(lumino_message::EditorAction::Moved(local));
                }
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
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            if let Some(local) = self.editor_local(pos) {
                                self.send_editor(lumino_message::EditorAction::Pressed {
                                    pos: local,
                                    shift: self.modifiers.shift_key(),
                                    ctrl: self.modifiers.control_key(),
                                });
                                self.editor_dragging = true;
                            }
                        } else if self.editor_dragging {
                            self.send_editor(lumino_message::EditorAction::Released);
                            self.editor_dragging = false;
                        }
                    }
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
    /// 把编辑器画布尺寸同步为「内容区」逻辑尺寸（iced 布局原本由 widget 写入）。
    fn sync_editor_canvas(&mut self, window: &Window) {
        let Some(root) = self.root.as_mut() else {
            return;
        };
        let ppp = window.scale_factor() as f32;
        let size = window.inner_size();
        let width = (size.width as f32 / ppp - lumino_ui_egui::layout::SIDEBAR_W).max(1.0);
        let height =
            (size.height as f32 / ppp - lumino_ui_egui::layout::TITLEBAR_H - root.toolbar.height)
                .max(1.0);
        root.editor
            .set_canvas_size(iced_core::Point::new(width, height));
        root.editor
            .set_canvas_offset(iced_core::Point::new(0.0, 0.0));
    }

    /// 窗口逻辑坐标 → 编辑器画布局部坐标（画布左上 = 左栏右缘 + 标题栏/工具栏下缘）。
    fn editor_local(&self, pos: Pos2) -> Option<lumino_message::Point2> {
        let left = lumino_ui_egui::layout::SIDEBAR_W;
        let toolbar_h = self
            .root
            .as_ref()
            .map(|root| root.toolbar.height)
            .unwrap_or(72.0);
        let top = lumino_ui_egui::layout::TITLEBAR_H + toolbar_h;
        if pos.x < left || pos.y < top {
            return None;
        }
        Some(lumino_message::Point2 {
            x: pos.x - left,
            y: pos.y - top,
        })
    }

    fn send_editor(&mut self, action: lumino_message::EditorAction) {
        tracing::debug!("native editor action: {action:?}");
        if let Some(root) = self.root.as_mut() {
            root.update(lumino_ui_core::Message::EditorAction(action));
            tracing::debug!(
                "native editor state: tool={:?} track={} count={} doc={}",
                root.editor.editor_state.tool,
                root.editor.editor_state.data.current_track,
                root.editor.editor_state.data.current_track_note_count(),
                root.editor.editor_state.data.document.is_some()
            );
        }
    }

    fn render(&mut self) {
        let (Some(window), Some(gpu)) = (self.window.as_ref(), self.gpu.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let ppp = window.scale_factor() as f32;
        let logical = Vec2::new(size.width as f32 / ppp, size.height as f32 / ppp);

        // gfx 编辑器画布 → egui 底层纹理（分离渲染线程，空场景先画背景）。
        if let Some(canvas) = self.canvas.as_ref() {
            if let Some(view) = canvas.render(size.width, size.height, logical.x, logical.y, ppp) {
                gpu.painter
                    .set_external_texture(&gpu.device, chrome::EDITOR_TEXTURE_ID, &view);
                chrome::set_base_texture(&self.ctx, Some(chrome::EDITOR_TEXTURE_ID));
            } else {
                gpu.painter
                    .clear_external_texture(chrome::EDITOR_TEXTURE_ID);
                chrome::set_base_texture(&self.ctx, None);
            }
        }

        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, logical)),
            events: std::mem::take(&mut self.events),
            time: self.start.map(|start| start.elapsed().as_secs_f64()),
            ..Default::default()
        };

        let mut root = self.root.take();
        let model = root
            .as_ref()
            .map(|root| root.egui_chrome_model_native(logical.x))
            .unwrap_or_else(|| demo_model(0));
        let notes = root
            .as_ref()
            .map(|root| root.editor.editor_state.data.current_track_note_count())
            .unwrap_or(0);
        let clicks = &mut self.clicks;
        let mut icons = std::mem::take(&mut self.icons);
        let output = self.ctx.run_ui(raw, |ui| {
            // 直接跑生产 egui 外壳（标题栏 / 工具栏 / 左右栏 / 状态栏）。
            chrome::show_bands(ui, &model, &mut icons);
            // 菜单 / 工具栏 / 左栏 / 状态栏 / 标题栏动作 → 生产 `Message`。
            if let Some(root) = root.as_mut() {
                let _ = root.apply_egui_chrome_actions(ui.ctx());
            }

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
                        ui.label(format!("notes {notes}"));
                    });
                });
        });
        self.root = root;
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
