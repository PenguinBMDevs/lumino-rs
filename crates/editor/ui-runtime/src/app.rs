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
    /// 最近一次同步到 gfx 渲染线程的音符世代（`track_notes_gen`）。
    note_gen: Option<u64>,
    start: Option<Instant>,
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    painter: Painter,
}

/// 主音轨音符描边编码：低 16 位 = 描边像素，高 16 位 = `track_enc`（`track_id + 1`）。
///
/// cull shader 用 `track_enc` 区分「主音轨 / 洋葱皮可见区间」：只写描边宽度
/// （`track_enc = 0`）会让所有音符被裁掉，画布看起来「什么都没画」。
fn note_border_width(track_id: usize) -> u32 {
    1 | (((track_id as u32) + 1) << 16)
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

    /// 全量上传所有轨道的音符实例（`OnionSkinStreamMsg` 会话）。
    fn upload_notes(
        &self,
        tracks: Vec<(usize, Vec<lumino_gfx::NoteInstance>)>,
        track_count: usize,
        current_track: usize,
    ) {
        use lumino_gfx::OnionSkinStreamMsg as Msg;

        let total: usize = tracks.iter().map(|(_, instances)| instances.len()).sum();
        tracing::info!(
            "native note upload: tracks={track_count} current={current_track} notes={total}"
        );
        // 注意：`TrackLayout` **不能**夹在全量会话中（渲染线程会按“交错”跳过），
        // 且 `Done` 会立即结束本轮 drain——因此布局补发放在 `Done` 之后，
        // 由渲染线程下一轮 drain 应用（段表在 `Chunk` 流中已按轨构建）。
        let mut ok = self.thread.send_onion_skin_msg(Msg::BeginSession);
        ok &= self.thread.send_onion_skin_msg(Msg::Reserve { total });
        for (track_id, instances) in tracks {
            ok &= self.thread.send_onion_skin_msg(Msg::Chunk {
                track_id,
                instances,
            });
        }
        ok &= self.thread.send_onion_skin_msg(Msg::SetViewState {
            current_track: current_track as u32 + 1,
            muted_tracks: Vec::new(),
            onion_visible: false,
            onion_range: (0, 0),
        });
        ok &= self.thread.send_onion_skin_msg(Msg::Done);
        ok &= self
            .thread
            .send_onion_skin_msg(Msg::TrackLayout { track_count });
        if !ok {
            tracing::warn!("native note upload: 有消息未送达（会话可能残缺）");
        }
    }

    /// 提交一帧并等待离屏纹理就绪，返回可采样的纹理视图。
    fn render(&self, params: lumino_gfx::RenderParams) -> Option<Arc<wgpu::TextureView>> {
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
        // 画布在窗口中的真实原点（左栏右缘 / 标题栏 + 工具栏下缘）：
        // 渲染线程的 `canvas_offset` 与 egui 的输入换算必须同源，否则音符
        // 会画在错误位置（或落到 scissor 之外）。
        root.editor.set_canvas_offset(iced_core::Point::new(
            lumino_ui_egui::layout::SIDEBAR_W,
            lumino_ui_egui::layout::TITLEBAR_H + root.toolbar.height,
        ));
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

        // 编辑器音符 → gfx 渲染线程（`track_notes_gen` 变化时全量上传）。
        if let (Some(root), Some(canvas)) = (self.root.as_ref(), self.canvas.as_ref()) {
            let generation = root.editor.editor_state.data.track_notes_gen;
            if self.note_gen != Some(generation) {
                let data = &root.editor.editor_state.data;
                let document = data.document.as_ref();
                let track_count = document.map_or(0, |doc| doc.track_count());
                let tracks: Vec<(usize, Vec<lumino_gfx::NoteInstance>)> = document
                    .map(|doc| {
                        (0..track_count)
                            .map(|track_id| {
                                // 高 16 位 = track_enc（track_id + 1）：cull shader 据此
                                // 判断「主音轨 / 洋葱皮可见区间」，低 16 位是描边像素。
                                // 只写描边宽度（track_enc = 0）会让所有音符被裁掉。
                                let border_width = note_border_width(track_id);
                                let instances = doc
                                    .track_notes(track_id)
                                    .iter()
                                    .map(|note| {
                                        lumino_gfx::NoteInstance::new(
                                            note.start_tick as f32,
                                            note.key,
                                            (note.end_tick.saturating_sub(note.start_tick)) as f32,
                                            [0.30, 0.62, 0.95, 1.0],
                                            border_width,
                                        )
                                    })
                                    .collect();
                                (track_id, instances)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                canvas.upload_notes(tracks, track_count, data.current_track);
                self.note_gen = Some(generation);
            }
        }

        // 渲染参数必须与编辑器视图同源（scroll / zoom / 画布原点 / 键数 / PPQ），
        // 否则音符与点击位置不一致，甚至被 canvas scissor 裁掉。
        let params = self
            .root
            .as_ref()
            .map(|root| {
                let es = &root.editor.editor_state;
                let view = &es.view;
                lumino_gfx::RenderParams {
                    viewport_size: (size.width.max(1), size.height.max(1)),
                    logical_size: (logical.x, logical.y),
                    scale_factor: ppp,
                    scroll: (view.scroll_x, view.scroll_y),
                    zoom: (view.zoom_x, view.zoom_y),
                    keyboard_width: view.keyboard_width,
                    ruler_height: view.ruler_height,
                    canvas_offset: (es.canvas.offset_x, es.canvas.offset_y),
                    canvas_size: (es.canvas.size_x, es.canvas.size_y),
                    ppq: view.ppq as f32,
                    max_key_index: view.visible_key_count.saturating_sub(1) as f32,
                    time_signatures: es.data.time_signatures.clone(),
                    // 与 egui 深色底色区分，便于真机确认底层纹理确实来自 gfx。
                    background_color: [0.07, 0.10, 0.16, 1.0],
                    color_bg: [0.07, 0.10, 0.16, 1.0],
                    content_dirty: true,
                    ..Default::default()
                }
            })
            .unwrap_or_else(|| lumino_gfx::RenderParams {
                viewport_size: (size.width.max(1), size.height.max(1)),
                logical_size: (logical.x, logical.y),
                scale_factor: ppp,
                canvas_size: (logical.x, logical.y),
                background_color: [0.07, 0.10, 0.16, 1.0],
                color_bg: [0.07, 0.10, 0.16, 1.0],
                content_dirty: true,
                ..Default::default()
            });

        // gfx 编辑器画布 → egui 底层纹理（分离渲染线程）。
        if let Some(canvas) = self.canvas.as_ref() {
            if let Some(view) = canvas.render(params) {
                gpu.painter
                    .set_external_texture(&gpu.device, chrome::EDITOR_TEXTURE_ID, &view);
                chrome::set_base_texture(&self.ctx, Some(chrome::EDITOR_TEXTURE_ID));
            } else {
                gpu.painter
                    .clear_external_texture(chrome::EDITOR_TEXTURE_ID);
                chrome::set_base_texture(&self.ctx, None);
            }
        }

        // 编辑器悬浮 √× 按钮（形状 / I2M / 曲线 / 画刷）。
        lumino_ui_egui::chrome::set_editor_buttons(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.editor_buttons_model())
                .unwrap_or_default(),
        );
        lumino_ui_egui::chrome::set_marquee(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.marquee_model())
                .unwrap_or_default(),
        );
        lumino_ui_egui::chrome::set_shape_preview(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.shape_preview_model())
                .unwrap_or_default(),
        );
        lumino_ui_egui::chrome::set_curve_preview(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.curve_preview_model())
                .unwrap_or_default(),
        );
        lumino_ui_egui::chrome::set_i2m_region(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.i2m_region_model())
                .unwrap_or_default(),
        );
        lumino_ui_egui::chrome::set_bend_buttons(
            &self.ctx,
            self.root
                .as_ref()
                .map(|root| root.bend_buttons_model())
                .unwrap_or_default(),
        );

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

#[cfg(test)]
mod tests {
    use super::note_border_width;

    /// `track_enc` 必须写进 border_width 高 16 位：cull shader 用它区分主音轨，
    /// 漏写会让画布上的所有音符被裁掉（真机表现为「画了但看不到」）。
    #[test]
    fn note_border_width_encodes_track_enc() {
        assert_eq!(note_border_width(0) >> 16, 1);
        assert_eq!(note_border_width(1) >> 16, 2);
        assert_eq!(note_border_width(0) & 0xffff, 1);
        assert_eq!(note_border_width(1) & 0xffff, 1);
    }
}
