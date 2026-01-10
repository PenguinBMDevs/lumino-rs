use std::sync::Arc;

use winit::{
    event::WindowEvent, event_loop::ControlFlow, keyboard::ModifiersState, window::WindowAttributes,
};

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
}

struct RunnerInner {
    // Wgpu instance
    gfx: lumino_gfx::Context,
    // Iced instance
    ui: lumino_ui::Host,

    window: Arc<winit::window::Window>,
    modifiers: ModifiersState,
    resized: bool,
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() {
            return;
        }

        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(winit::dpi::LogicalSize {
                width: 800,
                height: 600,
            })
            .with_inner_size(winit::dpi::LogicalSize {
                width: 1440,
                height: 900,
            })
            .with_title("Lumino")
            // The window should be invisible at first.
            // Make it visible when it's the right time.
            .with_visible(false);

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes
                // Disable native titlebar.
                .with_decorations(false)
                // Allows Windows to draw a shadow + frame on an undecorated window.
                // Improves UX when decorations is false.
                .with_undecorated_shadow(true);
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes = attributes
                // Make native titlebar transparent.
                .with_titlebar_transparent(true)
                // Allows the content to be integrated with native titlebar.
                .with_fullsize_content_view(true);
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("Create main window"),
        );

        let physical_size = window.inner_size();

        // Initialize wgpu
        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        ));

        // Initialize iced
        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            &gfx,
        );

        // You should change this if you want to render continuously
        event_loop.set_control_flow(ControlFlow::Wait);

        window.set_visible(true);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        self.inner = Some(RunnerInner {
            gfx,
            ui,
            window,
            modifiers: ModifiersState::default(),
            resized: false,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                if this.resized {
                    let size = this.window.inner_size();

                    this.ui.resize(size.width, size.height);
                    this.gfx.resize(size.width, size.height);

                    this.resized = false;
                }

                if this
                    .gfx
                    .with_frame(|a, b| this.ui.redraw_requested(a, b))
                    .is_err()
                {
                    this.window.request_redraw();
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                this.ui.cursor_moved(position);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                this.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(_) => {
                this.resized = true;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => (),
        }

        this.ui.handle_events(event, this.modifiers);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        let events = lumino_core::event::take_events();
        for event in events {
            use lumino_core::event::Event;
            match event {
                Event::Menu(r) => {
                    use lumino_core::event::menu::{Event::*, *};
                    match r {
                        File(r) => {
                            use file::Event::*;
                            match r {
                                Exit => event_loop.exit(),
                                _ => todo!(),
                            }
                        }
                        Edit(r) => {
                            use edit::Event::*;
                            match r {
                                _ => todo!(),
                            }
                        }
                        View(r) => {
                            use view::Event::*;
                            match r {
                                Theme(r) => this.ui.update_theme(r),
                            }
                        }
                        Help(r) => {
                            use help::Event::*;
                            match r {
                                _ => todo!(),
                            }
                        }
                    }
                }
                Event::Window(r) => {
                    use lumino_core::event::window::Event::*;
                    let w = &this.window;
                    match r {
                        Close => event_loop.exit(),
                        Drag => w.drag_window().expect("Drag window"),
                        Maximize => w.set_maximized(true),
                        Minimize => w.set_minimized(true),
                        ToggleMaximize => w.set_maximized(!w.is_maximized()),
                    }
                }
            }
        }
    }
}
