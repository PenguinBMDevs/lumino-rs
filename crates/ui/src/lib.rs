mod editor;
mod message;
mod resources;
mod root;
mod sidebar;
mod statusbar;
mod titlebar;
mod window;

pub(crate) use root::{Element, Message};
pub(crate) use lumino_core::storage::config;

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
}

impl Host {
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // Initialize iced
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
            root: root::Root::new(&ui_config.theme),
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: user_interface::Cache::new(),
            clipboard,
            viewport,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
    ) {
        // Draw iced on top
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

        // Update the mouse cursor
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            // Update the mouse cursor
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            } else {
                self.window.set_cursor_visible(false);
            }
        }

        // Draw the interface
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

        // Map window event to iced event
        if let Some(event) =
            conversion::window_event(event, self.window.scale_factor() as f32, modifiers)
        {
            // Convert touch events to mouse events for compatibility with widgets
            // that don't handle touch events (e.g., iced_aw MenuBar)
            let converted_events = convert_touch_to_mouse(event);
            self.events.extend(converted_events);
        }

        // If there are events pending
        if !self.events.is_empty() {
            // We process them
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

            // update our UI with any messages
            for message in messages {
                self.root.update(message);
            }

            // and request a redraw
            self.window.request_redraw();
        }
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
