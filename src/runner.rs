use std::sync::Arc;

use winit::{
    dpi,
    event::WindowEvent,
    event_loop::ControlFlow,
    keyboard::ModifiersState,
    window::WindowAttributes,
};

use super::storage;

// 从core导入MidiInfo
pub use lumino_core::MidiInfo;

#[derive(Default)]
pub struct Runner {
    inner: Option<RunnerInner>,
}

struct RunnerInner {
    gfx: lumino_gfx::Context,
    ui: lumino_ui::Host,
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
            .expect("初始化存储失败");

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
            .with_visible(false);

        if let (Some(x), Some(y)) = (ui_state.x, ui_state.y) && !ui_state.is_maximized {
            attributes = attributes.with_position(dpi::LogicalPosition { x, y });
        }

        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = attributes
                .with_decorations(false)
                .with_undecorated_shadow(true);
        }

        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attributes = attributes
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("创建窗口失败"),
        );

        let physical_size = window.inner_size();

        // 初始化wgpu
        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        ));

        // 初始化iced
        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            &config.ui,
            &gfx,
        );

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

                if this.gfx.with_frame(|a, b| this.ui.redraw_requested(a, b)).is_err() {
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
                                    // 打开MIDI文件
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("MIDI文件", &["mid", "midi"])
                                        .add_filter("所有文件", &["*"])
                                        .pick_file()
                                    {
                                        // 在后台线程中异步加载MIDI文件，避免阻塞UI
                                        tracing::info!("开始异步加载MIDI文件: {:?}", path);
                                        
                                        std::thread::spawn(move || {
                                            let rt = tokio::runtime::Runtime::new().unwrap();
                                            let result = rt.block_on(async {
                                                MidiInfo::from_path_with_progress(
                                                    path,
                                                    Some(&|progress| {
                                                        // 每10%报告一次进度，减少日志输出
                                                        if progress as u8 % 10 == 0 {
                                                            tracing::info!("解析进度: {:.0}%", progress);
                                                        }
                                                    }),
                                                ).await
                                            });
                                            
                                            // 记录加载结果
                                            match &result {
                                                Ok(info) => tracing::info!("MIDI加载完成: {} 个音轨, {} 个音符", 
                                                    info.track_count, info.total_notes),
                                                Err(e) => tracing::error!("MIDI加载失败: {}", e),
                                            }
                                        });
                                    }
                                }
                                _ => {
                                    tracing::debug!("未处理的文件事件: {:?}", r);
                                }
                            }
                        }
                        Edit(_r) => {
                            // TODO: 处理编辑事件
                        }
                        View(r) => {
                            use view::Event::*;
                            match r {
                                Theme(r) => {
                                    this.ui.update_theme(r.clone());
                                    this.storage.config.patch(|state| {
                                        state.ui.theme = r;
                                    });
                                }
                            }
                        }
                        Help(_r) => {
                            // TODO: 处理帮助事件
                        }
                    }
                }
                Event::Window(r) => {
                    use lumino_core::event::window::Event::*;
                    let w = &this.window;
                    match r {
                        Close => event_loop.exit(),
                        Drag => w.drag_window().expect("拖动窗口失败"),
                        Maximize => w.set_maximized(true),
                        Minimize => w.set_minimized(true),
                        ToggleMaximize => w.set_maximized(!w.is_maximized()),
                    }
                }
            }
        }

        if let Err(e) = this.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = this.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}
