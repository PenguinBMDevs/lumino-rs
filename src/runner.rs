use std::sync::Arc;
use tokio::sync::mpsc;

use winit::{
    dpi, event::WindowEvent, event_loop::ControlFlow, keyboard::ModifiersState,
    window::WindowAttributes,
};

use super::storage;
use lumino_core::event;

// 从core导入MidiInfo
pub use lumino_core::ParsedDms;
pub use lumino_core::ParsedMidi;

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
    current_midi: Option<ParsedMidi>,
    current_dms: Option<Arc<ParsedDms>>,
    progress: Option<(String, f64)>, // 消息和进度
    progress_rx: mpsc::UnboundedReceiver<(String, f64)>,
    progress_window: Option<Arc<winit::window::Window>>,
    progress_gfx: Option<lumino_gfx::Context>,
    progress_ui: Option<lumino_ui::Host>,
    progress_modifiers: ModifiersState,
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() {
            return;
        }

        let storage = storage::Storage::new().expect("初始化存储失败");

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

        if let (Some(x), Some(y)) = (ui_state.x, ui_state.y)
            && !ui_state.is_maximized
        {
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

        let window = Arc::new(event_loop.create_window(attributes).expect("创建窗口失败"));

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
            false, // not progress
        );

        // 创建进度channel
        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        lumino_core::midi::loader::set_progress_sender(progress_tx);

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
            current_midi: None,
            current_dms: None,
            progress: None,
            progress_rx,
            progress_window: None,
            progress_gfx: None,
            progress_ui: None,
            progress_modifiers: ModifiersState::default(),
        });
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        // 检查是否是进度窗口
        if let Some(ref progress_window) = this.progress_window
            && window_id == progress_window.id()
        {
            match event {
                WindowEvent::RedrawRequested => {
                    if let Some(ref mut progress_ui) = this.progress_ui
                        && let Some(ref progress_gfx) = this.progress_gfx
                        && progress_gfx
                            .with_frame(|a, b| progress_ui.redraw_requested(a, b))
                            .is_err()
                    {
                        progress_window.request_redraw();
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    if let Some(ref mut progress_ui) = this.progress_ui {
                        progress_ui.cursor_moved(position);
                    }
                }
                WindowEvent::ModifiersChanged(new_modifiers) => {
                    this.progress_modifiers = new_modifiers.state();
                }
                WindowEvent::Resized(size) => {
                    if let Some(ref mut progress_ui) = this.progress_ui {
                        progress_ui.resize(size.width, size.height);
                    }
                    if let Some(ref mut progress_gfx) = this.progress_gfx {
                        progress_gfx.resize(size.width, size.height);
                    }
                    progress_window.request_redraw();
                }
                WindowEvent::CloseRequested => {
                    // 关闭进度窗口时，结束进度
                    this.progress = None;
                    this.progress_window = None;
                    this.progress_gfx = None;
                    this.progress_ui = None;
                }
                _ => {
                    if let Some(ref mut progress_ui) = this.progress_ui {
                        progress_ui.handle_events(event, this.progress_modifiers);
                    }
                }
            }
            return;
        }

        // 主窗口事件
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
            WindowEvent::Touch(touch) => {
                this.ui.cursor_moved(touch.location);
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

        // 创建或销毁进度窗口
        if this.progress.is_some() && this.progress_window.is_none() {
            // 创建进度窗口
            let attributes = WindowAttributes::default()
                .with_inner_size(dpi::LogicalSize {
                    width: 500,
                    height: 200,
                })
                .with_title("MIDI 处理进度")
                .with_decorations(true)
                .with_visible(true);

            let progress_window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .expect("创建进度窗口失败"),
            );

            let physical_size = progress_window.inner_size();

            let progress_gfx = futures::executor::block_on(lumino_gfx::Context::new(
                progress_window.clone(),
                physical_size.width,
                physical_size.height,
            ));

            let progress_ui = lumino_ui::Host::new(
                progress_window.clone(),
                physical_size.width,
                physical_size.height,
                &this.storage.config.get().ui,
                &progress_gfx,
                true, // is_progress
            );

            this.progress_window = Some(progress_window);
            this.progress_gfx = Some(progress_gfx);
            this.progress_ui = Some(progress_ui);
        } else if this.progress.is_none() && this.progress_window.is_some() {
            // 销毁进度窗口
            this.progress_window = None;
            this.progress_gfx = None;
            this.progress_ui = None;
        }

        // 处理进度消息
        while let Ok((msg, progress)) = this.progress_rx.try_recv() {
            if progress >= 1.0 {
                this.progress = None;
                this.ui.update_progress(None);
                if let Some(ref mut progress_ui) = this.progress_ui {
                    progress_ui.update_progress(None);
                }
            } else {
                this.progress = Some((msg.clone(), progress));
                this.ui.update_progress(Some((msg.clone(), progress)));
                if let Some(ref mut progress_ui) = this.progress_ui {
                    progress_ui.update_progress(Some((msg, progress)));
                }
            }
            this.window.request_redraw();
            if let Some(ref progress_window) = this.progress_window {
                progress_window.request_redraw();
            }
        }

        // 处理窗口控制动作（最小化、最大化、关闭）
        if let Some(action) = this.ui.take_window_action() {
            use lumino_ui::window::TrafficAction;
            match action {
                TrafficAction::Minimize => {
                    this.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    if this.window.is_maximized() {
                        this.window.set_maximized(false);
                    } else {
                        this.window.set_maximized(true);
                    }
                }
                TrafficAction::Close => {
                    event_loop.exit();
                }
            }
        }

        // 处理标题栏拖动事件
        if this.ui.take_drag()
            && let Err(e) = this.window.drag_window()
        {
            tracing::warn!("拖动窗口失败: {}", e);
        }

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
                                Open => {
                                    // 打开音乐文件（支持 .mid, .midi, .lmpj, .dms）
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
                                        .add_filter("MIDI 文件", &["mid", "midi"])
                                        .add_filter("Lumino 项目", &["lmpj"])
                                        .add_filter("Domino 项目", &["dms"])
                                        .add_filter("所有文件", &["*"])
                                        .pick_file()
                                    {
                                        let extension = path
                                            .extension()
                                            .and_then(|ext| ext.to_str())
                                            .unwrap_or("")
                                            .to_ascii_lowercase();

                                        if extension == "dms" {
                                            // 加载 DMS 文件
                                            tracing::info!("开始后台加载 DMS 文件: {:?}", path);
                                            let path_clone = path.clone();
                                            tokio::spawn(async move {
                                                match lumino_core::midi::loader::load_dms(
                                                    path_clone,
                                                )
                                                .await
                                                {
                                                    Ok(parsed) => {
                                                        lumino_core::event::emit(event!(
                                                            Menu.File.DmsParsed(Arc::new(parsed))
                                                        ));
                                                    }
                                                    Err(e) => {
                                                        lumino_core::event::emit(event!(
                                                            Menu.File.DmsParseError(e)
                                                        ));
                                                    }
                                                }
                                            });
                                        } else {
                                            // 加载 MIDI 文件
                                            tracing::info!("开始后台加载 MIDI 文件: {:?}", path);
                                            let path_clone = path.clone();
                                            tokio::spawn(async move {
                                                match lumino_core::midi::loader::load_parsed_midi(
                                                    path_clone,
                                                )
                                                .await
                                                {
                                                    Ok(parsed) => {
                                                        lumino_core::event::emit(event!(
                                                            Menu.File.MidiParsed(parsed)
                                                        ));
                                                    }
                                                    Err(e) => {
                                                        lumino_core::event::emit(event!(
                                                            Menu.File.MidiParseError(e)
                                                        ));
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                                ImportMidi => {
                                    // 导入MIDI文件（仅支持 .mid 和 .midi）
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("MIDI文件", &["mid", "midi"])
                                        .add_filter("所有文件", &["*"])
                                        .pick_file()
                                    {
                                        // 在后台异步加载MIDI文件，不阻塞UI
                                        tracing::info!("开始后台导入MIDI文件: {:?}", path);

                                        let path_clone = path.clone();
                                        tokio::spawn(async move {
                                            match lumino_core::midi::loader::load_parsed_midi(
                                                path_clone,
                                            )
                                            .await
                                            {
                                                Ok(parsed) => {
                                                    lumino_core::event::emit(event!(
                                                        Menu.File.MidiParsed(parsed)
                                                    ));
                                                }
                                                Err(e) => {
                                                    lumino_core::event::emit(event!(
                                                        Menu.File.MidiParseError(e)
                                                    ));
                                                }
                                            }
                                        });
                                    }
                                }
                                Save => {
                                    // 保存当前加载的MIDI为 .lmpj 文件
                                    if let Some(parsed) = &this.current_midi {
                                        if let Some(save_path) = rfd::FileDialog::new()
                                            .add_filter("Lumino MIDI Project", &["lmpj"])
                                            .set_file_name(format!(
                                                "{}.lmpj",
                                                std::path::Path::new(&parsed.info.path)
                                                    .file_stem()
                                                    .unwrap_or_default()
                                                    .to_string_lossy()
                                            ))
                                            .save_file()
                                        {
                                            let parsed_clone = parsed.clone();
                                            tokio::spawn(async move {
                                                match lumino_core::midi::loader::save_to_lmpj(
                                                    &parsed_clone,
                                                    save_path.clone(),
                                                )
                                                .await
                                                {
                                                    Ok(()) => {
                                                        tracing::info!(
                                                            "MIDI保存成功: {:?}",
                                                            save_path
                                                        );
                                                    }
                                                    Err(e) => {
                                                        tracing::error!("MIDI保存失败: {}", e);
                                                    }
                                                }
                                            });
                                        }
                                    } else {
                                        tracing::warn!("没有加载的MIDI文件，无法保存");
                                    }
                                }
                                MidiLoaded(info) => {
                                    // 处理MIDI加载完成（旧的）
                                    tracing::info!("MIDI文件加载完成: {}", info);
                                    // TODO: 更新UI显示加载的MIDI信息
                                }
                                MidiLoadError(err) => {
                                    // 处理MIDI加载错误（旧的）
                                    tracing::error!("MIDI文件加载失败: {}", err);
                                    // TODO: 显示错误对话框或通知
                                }
                                MidiParsed(mut parsed) => {
                                    // 处理MIDI解析完成
                                    tracing::info!("MIDI文件解析完成: {}", parsed.info);

                                    // 立即释放 midi_data 内存，避免内存飙升
                                    // 仅在需要保存 LMPJ 时才会重新读取文件
                                    let _ = parsed.take_midi_data();
                                    tracing::debug!("MIDI原始数据已释放，仅保留元数据");

                                    this.current_midi = Some(parsed);
                                    // TODO: 更新UI显示加载的MIDI信息
                                }
                                MidiParseError(err) => {
                                    // 处理MIDI解析错误
                                    tracing::error!("MIDI文件解析失败: {}", err);
                                    // TODO: 显示错误对话框
                                }
                                DmsParsed(parsed) => {
                                    // 处理 DMS 文件解析完成
                                    tracing::info!("DMS 文件解析完成: {}", parsed.info);
                                    this.current_dms = Some(parsed);
                                    // TODO: 更新 UI 显示加载的 DMS 信息
                                }
                                DmsParseError(err) => {
                                    // 处理 DMS 文件解析错误
                                    tracing::error!("DMS 文件解析失败: {}", err);
                                    // TODO: 显示错误对话框
                                }
                                Close => {
                                    // TODO: 检查未保存更改，如有则弹出提示框
                                    // 强制卸载所有工程数据
                                    this.current_midi = None;
                                    this.current_dms = None;
                                    tracing::info!("工程已关闭");
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
                Event::Window(_r) => {}
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
