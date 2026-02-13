use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use winit::{
    dpi, event::WindowEvent, event_loop::ControlFlow, keyboard::ModifiersState,
    window::WindowAttributes,
};

use super::storage;
use lumino_core::event;

pub use lumino_core::ParsedDms;
pub use lumino_core::ParsedMidi;

const MIN_WINDOW_WIDTH: u32 = 800;
const MIN_WINDOW_HEIGHT: u32 = 600;
const PROGRESS_WINDOW_WIDTH: u32 = 500;
const PROGRESS_WINDOW_HEIGHT: u32 = 200;

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
    progress: Option<(String, f64)>,
    progress_rx: mpsc::UnboundedReceiver<(String, f64)>,
    progress_window: Option<Arc<winit::window::Window>>,
    progress_gfx: Option<lumino_gfx::Context>,
    progress_ui: Option<lumino_ui::Host>,
    progress_modifiers: ModifiersState,
    progress_completion_time: Option<Instant>, // 记录进度完成时间
}

impl winit::application::ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.inner.is_some() {
            return;
        }

        let inner = self.init_inner(event_loop);
        self.inner = Some(inner);
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

        if this.is_progress_window(window_id) {
            this.handle_progress_window_event(event);
            return;
        }

        this.handle_main_window_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let Some(this) = self.inner.as_mut() else {
            return;
        };

        this.process_progress_messages();  // 先处理进度消息
        this.update_progress_window(event_loop);  // 再更新窗口状态
        this.handle_window_actions(event_loop);
        this.process_core_events(event_loop);
        this.save_storage();
    }
}

impl Runner {
    fn init_inner(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) -> RunnerInner {
        let storage = storage::Storage::new().expect("初始化存储失败");

        let config = storage.config.get();
        let ui_state = storage.ui_state.get();

        let attributes = Self::build_window_attributes(ui_state);

        let window = Arc::new(event_loop.create_window(attributes).expect("创建窗口失败"));

        let physical_size = window.inner_size();

        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .expect("初始化图形上下文失败");

        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            &config.ui,
            &gfx,
            false,
        );

        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        lumino_core::midi::loader::set_progress_sender(progress_tx);

        event_loop.set_control_flow(ControlFlow::Wait);
        window.set_visible(true);

        #[cfg(target_os = "macos")]
        crate::platform::macos::init();

        RunnerInner {
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
            progress_completion_time: None,
        }
    }

    fn build_window_attributes(
        ui_state: &lumino_core::storage::ui_state::UiState,
    ) -> WindowAttributes {
        let mut attributes = WindowAttributes::default()
            .with_min_inner_size(dpi::LogicalSize {
                width: MIN_WINDOW_WIDTH,
                height: MIN_WINDOW_HEIGHT,
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

        attributes
    }
}

impl RunnerInner {
    fn is_progress_window(&self, window_id: winit::window::WindowId) -> bool {
        self.progress_window
            .as_ref()
            .is_some_and(|w| w.id() == window_id)
    }

    fn handle_progress_window_event(&mut self, event: WindowEvent) {
        let Some(progress_window) = self.progress_window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                self.handle_progress_redraw(&progress_window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref mut progress_ui) = self.progress_ui {
                    progress_ui.cursor_moved(position);
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.progress_modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_progress_resize(size, &progress_window);
            }
            WindowEvent::CloseRequested => {
                self.close_progress_window();
            }
            _ => {
                if let Some(ref mut progress_ui) = self.progress_ui {
                    progress_ui.handle_events(event, self.progress_modifiers);
                }
            }
        }
    }

    fn handle_progress_redraw(&mut self, progress_window: &Arc<winit::window::Window>) {
        if let Some(ref mut progress_ui) = self.progress_ui
            && let Some(ref progress_gfx) = self.progress_gfx
            && progress_gfx
                .with_frame(|a, b| progress_ui.redraw_requested(a, b))
                .is_err()
        {
            progress_window.request_redraw();
        }
    }

    fn handle_progress_resize(
        &mut self,
        size: winit::dpi::PhysicalSize<u32>,
        progress_window: &Arc<winit::window::Window>,
    ) {
        if let Some(ref mut progress_ui) = self.progress_ui {
            progress_ui.resize(size.width, size.height);
        }
        if let Some(ref mut progress_gfx) = self.progress_gfx {
            progress_gfx.resize(size.width, size.height);
        }
        progress_window.request_redraw();
    }

    fn close_progress_window(&mut self) {
        self.progress = None;
        self.progress_window = None;
        self.progress_gfx = None;
        self.progress_ui = None;
        self.progress_completion_time = None; // 同时重置完成时间
    }

    fn handle_main_window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                self.handle_main_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.cursor_moved(position);
            }
            WindowEvent::Touch(touch) => {
                self.ui.cursor_moved(touch.location);
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_main_resize(size);
            }
            WindowEvent::Moved(pos) => {
                self.storage.ui_state.patch(|state| {
                    state.x = Some(pos.x);
                    state.y = Some(pos.y);
                });
            }
            WindowEvent::CloseRequested => {
                self.window.request_redraw();
            }
            _ => (),
        }

        self.ui.handle_events(event, self.modifiers);
    }

    fn handle_main_redraw(&mut self) {
        if self.resized {
            let size = self.window.inner_size();
            self.ui.resize(size.width, size.height);
            self.gfx.resize(size.width, size.height);
            self.resized = false;
        }

        if self
            .gfx
            .with_frame(|a, b| self.ui.redraw_requested(a, b))
            .is_err()
        {
            self.window.request_redraw();
        };
    }

    fn handle_main_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.storage.ui_state.patch(|state| {
            state.w = size.width;
            state.h = size.height;
            state.is_maximized = self.window.is_maximized();
        });
        self.resized = true;
    }

    fn update_progress_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        // 检查是否应该关闭完成状态的进度窗口（在显示一段时间后）
        if let Some(completion_time) = self.progress_completion_time {
            // 如果进度已经完成超过1.5秒，则关闭窗口
            if completion_time.elapsed() > Duration::from_millis(1500) {
                self.close_progress_window();
                self.progress_completion_time = None; // 重置完成时间
            }
        }
        
        if self.progress.is_some() && self.progress_window.is_none() {
            self.create_progress_window(event_loop);
        } else if self.progress.is_none() && self.progress_window.is_some() {
            self.close_progress_window();
        }
    }

    fn create_progress_window(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_inner_size(dpi::LogicalSize {
                width: PROGRESS_WINDOW_WIDTH,
                height: PROGRESS_WINDOW_HEIGHT,
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
        ))
        .expect("初始化进度窗口图形上下文失败");

        let progress_ui = lumino_ui::Host::new(
            progress_window.clone(),
            physical_size.width,
            physical_size.height,
            &self.storage.config.get().ui,
            &progress_gfx,
            true,
        );

        self.progress_window = Some(progress_window);
        self.progress_gfx = Some(progress_gfx);
        self.progress_ui = Some(progress_ui);
    }

    fn process_progress_messages(&mut self) {
        while let Ok((msg, progress)) = self.progress_rx.try_recv() {
            self.handle_progress_message(msg, progress);
        }
    }

    fn handle_progress_message(&mut self, msg: String, progress: f64) {
        if progress >= 1.0 {
            // 当进度达到1.0时，记录完成时间，但不立即关闭窗口
            // 这样可以让用户看到完成状态
            self.progress = Some((msg.clone(), 1.0)); // 确保进度显示为100%
            self.progress_completion_time = Some(Instant::now()); // 记录完成时间
            self.ui.update_progress(Some((msg.clone(), 1.0)));
            if let Some(ref mut progress_ui) = self.progress_ui {
                progress_ui.update_progress(Some((msg, 1.0)));
            }
        } else {
            // 更新进度完成时间为None，表示仍在进行中
            self.progress_completion_time = None;
            self.progress = Some((msg.clone(), progress));
            self.ui.update_progress(Some((msg.clone(), progress)));
            if let Some(ref mut progress_ui) = self.progress_ui {
                progress_ui.update_progress(Some((msg, progress)));
            }
        }
        self.window.request_redraw();
        if let Some(ref progress_window) = self.progress_window {
            progress_window.request_redraw();
        }
    }

    fn handle_window_actions(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(action) = self.ui.take_window_action() {
            use lumino_ui::window::TrafficAction;
            match action {
                TrafficAction::Minimize => {
                    self.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    let is_maximized = self.window.is_maximized();
                    self.window.set_maximized(!is_maximized);
                }
                TrafficAction::Close => {
                    event_loop.exit();
                }
            }
        }

        if self.ui.take_drag()
            && let Err(e) = self.window.drag_window()
        {
            tracing::warn!("拖动窗口失败: {}", e);
        }
    }

    fn process_core_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let events = lumino_core::event::take_events();
        for event in events {
            self.handle_core_event(event_loop, event);
        }
    }

    fn handle_core_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: lumino_core::event::Event,
    ) {
        use lumino_core::event::Event;

        match event {
            Event::Menu(menu_event) => {
                self.handle_menu_event(event_loop, menu_event);
            }
            Event::Window(_) => {}
        }
    }

    fn handle_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        menu_event: lumino_core::event::menu::Event,
    ) {
        use lumino_core::event::menu::Event::*;

        match menu_event {
            File(file_event) => {
                self.handle_file_menu_event(event_loop, file_event);
            }
            Edit(_) => {
                // TODO: 处理编辑事件
            }
            View(view_event) => {
                self.handle_view_menu_event(view_event);
            }
            Help(_) => {
                // TODO: 处理帮助事件
            }
        }
    }

    fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_core::event::menu::file::Event,
    ) {
        use lumino_core::event::menu::file::Event::*;

        match file_event {
            Exit => event_loop.exit(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            Save => self.handle_save_file(),
            MidiLoaded(info) => {
                tracing::info!("MIDI文件加载完成: {}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI文件加载失败: {}", err);
            }
            MidiParsed(mut parsed) => {
                tracing::info!("MIDI文件解析完成: {}", parsed.info);
                let _ = parsed.take_midi_data();
                tracing::debug!("MIDI原始数据已释放，仅保留元数据");
                self.current_midi = Some(parsed);
            }
            MidiParseError(err) => {
                tracing::error!("MIDI文件解析失败: {}", err);
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成: {}", parsed.info);
                self.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败: {}", err);
            }
            Close => {
                self.current_midi = None;
                self.current_dms = None;
                tracing::info!("工程已关闭");
            }
            _ => {
                tracing::debug!("未处理的文件事件: {:?}", file_event);
            }
        }
    }

    fn handle_open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if extension == "dms" {
            self.load_dms_file(path);
        } else if extension == "lmpj" {
            // LMPJ文件也是MIDI类型的，使用MIDI加载器
            self.load_midi_file(path);
        } else {
            self.load_midi_file(path);
        }
    }

    fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件: {:?}", path);
        tokio::spawn(async move {
            match lumino_core::midi::loader::load_dms(path).await {
                Ok(parsed) => {
                    lumino_core::event::emit(event!(Menu.File.DmsParsed(Arc::new(parsed))));
                }
                Err(e) => {
                    lumino_core::event::emit(event!(Menu.File.DmsParseError(e)));
                }
            }
        });
    }

    fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件: {:?}", path);
        tokio::spawn(async move {
            match lumino_core::midi::loader::load_parsed_midi(path).await {
                Ok(parsed) => {
                    lumino_core::event::emit(event!(Menu.File.MidiParsed(parsed)));
                }
                Err(e) => {
                    lumino_core::event::emit(event!(Menu.File.MidiParseError(e)));
                }
            }
        });
    }

    fn handle_import_files(&self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms", "ldms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("Lumino DMS 项目", &["ldms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        tracing::info!("开始后台导入文件: {:?}", path);
        tokio::spawn(async move {
            match extension.as_str() {
                "dms" => {
                    match lumino_core::midi::loader::load_dms(path).await {
                        Ok(parsed) => {
                            lumino_core::event::emit(event!(Menu.File.DmsParsed(Arc::new(parsed))));
                        }
                        Err(e) => {
                            lumino_core::event::emit(event!(Menu.File.DmsParseError(e)));
                        }
                    }
                }
                "ldms" | "lmpj" => {
                    // LDMS和LMPJ都使用MIDI加载器，因为它们都是序列化的项目文件
                    match lumino_core::midi::loader::load_parsed_midi(path).await {
                        Ok(parsed) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParsed(parsed)));
                        }
                        Err(e) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParseError(e)));
                        }
                    }
                }
                _ => { // 默认处理MIDI文件
                    match lumino_core::midi::loader::load_parsed_midi(path).await {
                        Ok(parsed) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParsed(parsed)));
                        }
                        Err(e) => {
                            lumino_core::event::emit(event!(Menu.File.MidiParseError(e)));
                        }
                    }
                }
            }
        });
    }

    fn handle_save_file(&mut self) {
        // 检查是否加载了MIDI文件
        if let Some(parsed_midi) = &self.current_midi {
            let Some(save_path) = rfd::FileDialog::new()
                .add_filter("Lumino MIDI Project", &["lmpj"])
                .set_file_name(format!(
                    "{}.lmpj",
                    std::path::Path::new(&parsed_midi.info.path)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                ))
                .save_file()
            else {
                return;
            };

            let parsed_clone = parsed_midi.clone();
            tokio::spawn(async move {
                match lumino_core::midi::loader::save_to_lmpj(&parsed_clone, save_path.clone()).await {
                    Ok(()) => {
                        tracing::info!("MIDI保存成功: {:?}", save_path);
                    }
                    Err(e) => {
                        tracing::error!("MIDI保存失败: {}", e);
                    }
                }
            });
            return;
        }

        // 检查是否加载了DMS文件
        if let Some(parsed_dms) = &self.current_dms {
            let Some(save_path) = rfd::FileDialog::new()
                .add_filter("Lumino DMS Project", &["ldms"])  // 新增Lumino DMS项目格式
                .set_file_name(format!(
                    "{}.ldms",
                    std::path::Path::new(&parsed_dms.info.path)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                ))
                .save_file()
            else {
                return;
            };

            let parsed_clone = parsed_dms.clone();
            tokio::spawn(async move {
                match lumino_core::midi::loader::save_dms_to_ldms(&parsed_clone, save_path.clone()).await {
                    Ok(()) => {
                        tracing::info!("DMS保存成功: {:?}", save_path);
                    }
                    Err(e) => {
                        tracing::error!("DMS保存失败: {}", e);
                    }
                }
            });
            return;
        }

        tracing::warn!("没有加载的文件，无法保存");
    }

    fn handle_view_menu_event(&mut self, view_event: lumino_core::event::menu::view::Event) {
        use lumino_core::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.ui.update_theme(theme.clone());
                self.storage.config.patch(|state| {
                    state.ui.theme = theme;
                });
            }
        }
    }

    fn save_storage(&mut self) {
        if let Err(e) = self.storage.config.save() {
            tracing::warn!("保存配置失败: {e}");
        }
        if let Err(e) = self.storage.ui_state.save() {
            tracing::warn!("保存UI状态失败: {e}");
        }
    }
}
