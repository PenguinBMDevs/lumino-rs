use std::sync::Arc;

use winit::{
    dpi,
    event::WindowEvent,
    event_loop::ControlFlow,
    keyboard::ModifiersState,
    window::WindowAttributes,
};

use super::storage;

// 从core crate导入MidiInfo
pub use lumino_core::MidiInfo;

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
}

struct RunnerInner {
    // Wgpu instance
    gfx: lumino_gfx::Context,
    // Iced instance
    ui: lumino_ui::Host,
    // Storage system
    storage: storage::Storage,

    window: Arc<winit::window::Window>,
    modifiers: ModifiersState,
    resized: bool,
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() {
            return;
        }

        let storage = storage::Storage::new()
            // `expect()` is a temporary solution. remove it in the future.
            .expect("Initialize storage");

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(dpi::LogicalSize {
                width: 800,
                height: 600,
            })
            .with_inner_size(dpi::LogicalSize {
                width: ui_state.w,
                height: ui_state.h,
            })
            .with_maximized(ui_state.is_maximized)
            .with_title("Lumino")
            // The window should be invisible at first.
            // Make it visible when it's the right time.
            .with_visible(false);

        if
            let (Some(x), Some(y)) = (ui_state.x, ui_state.y) &&
            !ui_state.is_maximized
        {
            attributes = attributes
                .with_position(dpi::LogicalPosition {
                    x, y
                });
        }

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
            &config.ui,
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
            storage,
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
            WindowEvent::Resized(size) => {
                this.storage.ui_state.patch(|state| {
                    state.w = size.width;
                    state.h = size.height;
                    state.is_maximized = this.window.is_maximized();
                });
                this.resized = true;
            }
            WindowEvent::Moved(pos) => {
                this.storage.ui_state.patch(|state| {
                    state.x = Some(pos.x);
                    state.y = Some(pos.y);
                });
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
                                Open | ImportMidi => {
                                    // Open file dialog for MIDI files
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("MIDI files", &["mid", "midi"])
                                        .add_filter("All files", &["*"])
                                        .pick_file()
                                    {
                                        // 异步解析MIDI文件，使用tracing输出进度
                                        // 使用 futures::executor::block_on 在同步上下文中运行异步代码
                                        let result = futures::executor::block_on(async {
                                            MidiInfo::from_path_with_progress(
                                                path,
                                                Some(&|progress| {
                                                    tracing::info!("MIDI解析进度: {:.1}%", progress);
                                                }),
                                            ).await
                                        });
                                        
                                        match result {
                                            Ok(info) => {
                                                tracing::info!("Loaded MIDI file:\n{}", info);
                                                // TODO: Store the MIDI info and use it in the application
                                                // For now, just log it
                                            }
                                            Err(e) => {
                                                tracing::error!("Failed to parse MIDI file: {}", e);
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    tracing::debug!("Unhandled file event: {:?}", r);
                                }
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
                                Theme(r) => {
                                    this.ui.update_theme(r.clone());
                                    this.storage.config.patch(|state| {
                                        state.ui.theme = r;
                                    });
                                },
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

        // 1. this is idempotent, no need to check the `dirty` state.
        // 2. results should be actually handled in the future.
        if let Err(e) = this.storage.config.save() {
            tracing::warn!("failed to save config: {e}");
        }
        if let Err(e) = this.storage.ui_state.save() {
            tracing::warn!("failed to save ui_state: {e}");
        }
    }
}
