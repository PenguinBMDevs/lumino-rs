mod editor;
pub mod message;
mod resources;
mod root;
mod sidebar;
mod statusbar;
mod titlebar;
pub mod window;

pub(crate) use lumino_core::storage::config;
pub(crate) use root::{Element, Message};

use std::{sync::Arc, time::Instant};

use iced_wgpu::{
    Engine, Renderer,
    graphics::{Shell, Viewport},
    wgpu,
};
use lumino_gfx::NoteRenderer;

use iced_winit::{
    Clipboard, conversion,
    runtime::user_interface::{self, UserInterface},
    winit,
};

use iced_core::{Event, Font, Pixels, Size, Theme, mouse, renderer, touch};

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
pub struct Host {
    window: Arc<winit::window::Window>,
    root: root::Root,
    renderer: Renderer,
    events: Vec<Event>,
    cursor: mouse::Cursor,
    cache: user_interface::Cache,
    clipboard: Clipboard,
    viewport: Viewport,
    pending_window_action: Option<window::TrafficAction>,
    pending_drag: bool,
    /// wgpu 音符渲染器
    note_renderer: NoteRenderer,
    /// 逻辑光标位置（用于音符预览）
    cursor_position: Option<iced_core::Point>,
}

impl Host {
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 初始化 iced
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                Shell::headless(),
            );
            Renderer::new(engine, Font::default(), Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: if is_progress {
                root::Root::new_progress(&ui_config.theme)
            } else {
                root::Root::new(&ui_config.theme)
            },
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: user_interface::Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            cursor_position: None,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    /// 处理重绘请求 - 先渲染 iced UI，再用 wgpu 渲染音符
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 第一步：渲染 iced UI
        self.render_iced_ui(frame, view);

        // 第二步：用 wgpu 渲染音符（在 UI 之上）
        self.render_notes(frame, view, gfx);
    }

    /// 渲染 iced UI
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        // 先构建 view（这会借用 root）
        let root_view = self.root.view();
        
        let mut interface = UserInterface::build(
            root_view,
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );

        let mut messages = Vec::new();
        let (state, _) = interface.update(
            &[Event::Window(iced_core::window::Event::RedrawRequested(
                Instant::now(),
            ))],
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        // 绘制界面（在释放 root 借用之前）
        let theme = self.root.theme();
        interface.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style::default(),
            self.cursor,
        );
        self.cache = interface.into_cache();

        self.renderer
            .present(None, frame.texture.format(), texture_view, &self.viewport);

        // 处理消息（在 interface 被释放之后，root 不再被借用）
        for message in messages {
            self.root.update(message);
        }

        // 更新鼠标光标
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            } else {
                self.window.set_cursor_visible(false);
            }
        }
    }

    /// 使用 wgpu 渲染音符
    fn render_notes(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 同步光标位置到 editor
        self.root.update_editor_cursor(self.cursor_position);

        // 检查鼠标是否在 Canvas 区域内（严格检查，防止覆盖菜单）
        if let Some(pos) = self.cursor_position {
            let canvas_offset = self.root.editor.canvas_offset;
            let canvas_size = self.root.editor.canvas_size;
            
            // 检查是否在 Canvas 水平范围内
            if pos.x < canvas_offset.x || pos.x > canvas_offset.x + canvas_size.x {
                return;
            }
            // 检查是否在 Canvas 垂直范围内（包含顶部安全区域）
            if pos.y < canvas_offset.y + 40.0 || pos.y > canvas_offset.y + canvas_size.y {
                return;
            }
        } else {
            return; // 没有鼠标位置，不渲染
        }

        // 获取需要绘制的音符实例
        let instances = self.root.get_note_instances();
        if instances.is_empty() {
            return;
        }

        // 创建命令编码器
        let mut encoder = gfx.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("note_render_encoder"),
        });

        // 开始渲染通道
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // 在已有内容（UI）之上绘制
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // 使用逻辑尺寸绘制音符（与 iced 坐标系一致）
        let logical_size = self.viewport.logical_size();
        
        // 计算 Canvas 区域的裁剪矩形（限制音符只在卷帘内显示）
        // 转换为物理像素坐标用于 scissor rect
        let scale = self.viewport.scale_factor();
        let canvas_offset = self.root.editor.canvas_offset;
        let canvas_size = self.root.editor.canvas_size;
        
        let scissor_x = (canvas_offset.x * scale) as u32;
        let scissor_y = (canvas_offset.y * scale) as u32;
        let scissor_width = (canvas_size.x * scale) as u32;
        let scissor_height = (canvas_size.y * scale) as u32;
        
        self.note_renderer.draw(
            &mut render_pass,
            &instances,
            &gfx.device,
            &gfx.queue,
            (logical_size.width, logical_size.height),
            Some((scissor_x, scissor_y, scissor_width, scissor_height)),
        );

        // 释放 render_pass，提交命令
        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let logical_pos = conversion::cursor_position(position, self.viewport.scale_factor());
        self.cursor = mouse::Cursor::Available(logical_pos);
        // 存储逻辑坐标（与 iced 一致）
        self.cursor_position = Some(logical_pos);
    }

    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::WindowEvent::*;

        match event {
            Resized(_) => self
                .root
                .update(message::Window::maximized(self.window.is_maximized())),
            Focused(r) => self.root.update(message::Window::focused(r)),
            _ => (),
        }

        // 将窗口事件映射到 iced 事件
        if let Some(event) =
            conversion::window_event(event, self.window.scale_factor() as f32, modifiers)
        {
            let converted_events = convert_touch_to_mouse(event);
            self.events.extend(converted_events);
        }

        // 处理事件
        if !self.events.is_empty() {
            let mut interface = UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                std::mem::take(&mut self.cache),
                &mut self.renderer,
            );

            let mut messages = Vec::new();

            let _ = interface.update(
                &self.events,
                self.cursor,
                &mut self.renderer,
                &mut self.clipboard,
                &mut messages,
            );

            self.events.clear();
            self.cache = interface.into_cache();

            // 应用消息
            for message in messages {
                if let message::Message::Window(window::Event::TrafficAction(action)) = &message {
                    self.pending_window_action = Some(action.clone());
                }
                if let message::Message::Window(window::Event::Drag) = &message {
                    self.pending_drag = true;
                }
                self.root.update(message);
            }

            self.window.request_redraw();
        }
    }

    /// 获取并清除待处理的窗口动作
    pub fn take_window_action(&mut self) -> Option<window::TrafficAction> {
        self.pending_window_action.take()
    }

    /// 获取并清除待处理的拖动标志
    pub fn take_drag(&mut self) -> bool {
        let drag = self.pending_drag;
        self.pending_drag = false;
        drag
    }

    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
    }

    pub fn update_theme(&mut self, theme: String) {
        self.root.update(message::Window::theme(theme));
    }
}

/// 将触摸事件转换为鼠标事件（兼容性处理）
fn convert_touch_to_mouse(event: Event) -> Vec<Event> {
    match event {
        Event::Touch(touch_event) => match touch_event {
            touch::Event::FingerPressed { position, .. } => {
                vec![
                    event,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerLifted { position, .. } => {
                vec![
                    event,
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerMoved { position, .. } => {
                vec![event, Event::Mouse(mouse::Event::CursorMoved { position })]
            }
            _ => vec![event],
        },
        _ => vec![event],
    }
}
