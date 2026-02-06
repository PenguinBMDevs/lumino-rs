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

use iced_winit::{
    Clipboard, conversion,
    runtime::user_interface::{self, UserInterface},
    winit,
};

use iced_core::{Event, Font, Pixels, Size, Theme, mouse, renderer, touch};

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
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    pub fn redraw_requested(&mut self, frame: &wgpu::SurfaceTexture, view: &wgpu::TextureView) {
        // 在顶层绘制 iced
        let mut interface = UserInterface::build(
            self.root.view(),
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );

        let (state, _) = interface.update(
            &[Event::Window(iced_core::window::Event::RedrawRequested(
                Instant::now(),
            ))],
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut Vec::new(),
        );

        // 更新鼠标光标
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            // 更新鼠标光标
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            } else {
                self.window.set_cursor_visible(false);
            }
        }

        // 绘制界面
        interface.draw(
            &mut self.renderer,
            &self.root.theme(),
            &renderer::Style::default(),
            self.cursor,
        );
        self.cache = interface.into_cache();

        self.renderer
            .present(None, frame.texture.format(), view, &self.viewport);
    }

    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.cursor = mouse::Cursor::Available(conversion::cursor_position(
            position,
            self.viewport.scale_factor(),
        ));
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
            // Convert touch events to mouse events for compatibility with widgets
            // that don't handle touch events (e.g., iced_aw MenuBar)
            let converted_events = convert_touch_to_mouse(event);
            self.events.extend(converted_events);
        }

        // 如果有待处理的事件
        if !self.events.is_empty() {
            // 我们处理这些事件
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

            // 使用任意消息更新我们的 UI
            for message in messages {
                // 检查是否是窗口控制动作
                if let message::Message::Window(window::Event::TrafficAction(action)) = &message {
                    self.pending_window_action = Some(action.clone());
                }
                // 检查是否是拖动事件
                if let message::Message::Window(window::Event::Drag) = &message {
                    self.pending_drag = true;
                }
                self.root.update(message);
            }

            // 并请求重新绘制
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

/// Converts touch events to mouse events for compatibility with widgets
/// that only handle mouse events (e.g., iced_aw MenuBar).
/// Returns a vector of events (either the original event + converted mouse event,
/// or just the original event if no conversion is needed).
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
                vec![
                    event,
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            _ => vec![event],
        },
        _ => vec![event],
    }
}
