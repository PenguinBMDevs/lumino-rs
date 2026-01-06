use std::sync::Arc;

use winit::{
    event::WindowEvent, event_loop::ControlFlow, keyboard::ModifiersState, window::WindowAttributes
};

pub enum Runner {
    Loading,
    Ready {
        // Wgpu instance
        gfx: lumino_gfx::Context,
        // Iced instance
        ui: lumino_ui::Host,

        window: Arc<winit::window::Window>,
        modifiers: ModifiersState,
        resized: bool,
    },
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        match self {
            Self::Loading => (),
            _ => return,
        }

        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(winit::dpi::LogicalSize {
                width: 800,
                height: 600,
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
                .expect("Create main window")
        );

        #[cfg(target_os = "windows")]
        {
            // window.
        }

        let physical_size = window.inner_size();

        // Initialize wgpu
        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height
        ));

        // Initialize iced
        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            &gfx
        );

        // You should change this if you want to render continuously
        event_loop.set_control_flow(ControlFlow::Wait);

        window.set_visible(true);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        *self = Self::Ready {
            gfx,
            ui,
            window,
            modifiers: ModifiersState::default(),
            resized: false,
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Self::Ready {
            gfx,
            ui,
            window,
            modifiers,
            resized,
        } = self else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                if *resized {
                    let size = window.inner_size();

                    ui.resize(
                        size.width,
                        size.height,
                    );
                    gfx.resize(
                        size.width,
                        size.height
                    );

                    *resized = false;
                }

                if gfx.with_frame(|a, b| ui.redraw_requested(a, b)).is_err() {
                    window.request_redraw();
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                ui.cursor_moved(position);
            },
            WindowEvent::ModifiersChanged(new_modifiers) => {
                *modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(_) => {
                *resized = true;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            _ => ()
        }

        ui.handle_events(event, *modifiers);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Self::Ready {
            window,
            ..
        } = self else {
            return;
        };

        let events = lumino_core::event::take_events();
        for event in events {
            use lumino_core::event::Event;
            match event {
                Event::Menu(r) => {
                    use lumino_core::event::menu::{
                        *,
                        Event::*,
                    };
                    match r {
                        File(r) => {
                            use file::Event::*;
                            match r {
                                Exit => event_loop.exit(),
                                _ => todo!(),
                            }
                        },
                        Edit(r) => {
                            use edit::Event::*;
                            match r {
                                _ => todo!(),

                            }
                        },
                        View(r) => {
                            use view::Event::*;
                            match r {
                                _ => todo!(),

                            }
                        },
                        Help(r) => {
                            use help::Event::*;
                            match r {
                                _ => todo!(),

                            }
                        }
                    }
                },
                Event::Window(r) => {
                    use lumino_core::event::window::Event::*;
                    match r {
                        Close => event_loop.exit(),
                        Drag => {
                            window.drag_window().expect("Drag window")
                        },
                        Maximize => window.set_maximized(true),
                        Minimize => window.set_minimized(true),
                        ToggleMaximize => window.set_maximized(
                            !window.is_maximized()
                        ),
                    }
                }
            }
        }
    }
}
