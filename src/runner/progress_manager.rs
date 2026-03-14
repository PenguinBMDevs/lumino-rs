use std::sync::Arc;
use tokio::sync::mpsc;
use winit::{dpi, event::WindowEvent, window::WindowAttributes};

pub const PROGRESS_WINDOW_WIDTH: u32 = 500;
pub const PROGRESS_WINDOW_HEIGHT: u32 = 200;

/// 进度窗口管理器
///
/// 负责管理进度窗口的创建、更新和销毁
pub struct ProgressManager {
    /// 进度接收器
    rx: mpsc::UnboundedReceiver<(String, f64)>,
    /// 当前进度
    progress: Option<(String, f64)>,
    /// 进度窗口
    window: Option<Arc<winit::window::Window>>,
    /// 进度窗口图形上下文
    gfx: Option<lumino_gfx::Context>,
    /// 进度窗口 UI
    ui: Option<lumino_ui::Host>,
    /// 修饰键状态
    modifiers: winit::keyboard::ModifiersState,
}

impl ProgressManager {
    /// 创建新的进度管理器
    pub fn new() -> (Self, mpsc::UnboundedSender<(String, f64)>) {
        let (tx, rx) = mpsc::unbounded_channel();

        let manager = Self {
            rx,
            progress: None,
            window: None,
            gfx: None,
            ui: None,
            modifiers: winit::keyboard::ModifiersState::default(),
        };

        (manager, tx)
    }

    /// 检查是否是进度窗口的 ID
    pub fn is_progress_window(&self, window_id: winit::window::WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == window_id)
    }

    /// 获取当前进度
    pub fn progress(&self) -> Option<&(String, f64)> {
        self.progress.as_ref()
    }

    /// 处理进度窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                self.handle_redraw(&window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref mut ui) = self.ui {
                    ui.cursor_moved(position);
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size, &window);
            }
            WindowEvent::CloseRequested => {
                self.close();
            }
            _ => {
                if let Some(ref mut ui) = self.ui {
                    ui.handle_events(event, self.modifiers);
                }
            }
        }
    }

    /// 处理重绘
    fn handle_redraw(&mut self, window: &Arc<winit::window::Window>) {
        if let Some(ref mut ui) = self.ui
            && let Some(ref gfx) = self.gfx
            && gfx
                .with_frame(|frame, view| ui.redraw_requested(frame, view, gfx))
                .is_err()
        {
            window.request_redraw();
        }
    }

    /// 处理大小改变
    fn handle_resize(
        &mut self,
        size: winit::dpi::PhysicalSize<u32>,
        window: &Arc<winit::window::Window>,
    ) {
        if let Some(ref mut ui) = self.ui {
            ui.resize(size.width, size.height);
        }
        if let Some(ref mut gfx) = self.gfx {
            gfx.resize(size.width, size.height);
        }
        window.request_redraw();
    }

    /// 关闭进度窗口
    pub fn close(&mut self) {
        self.progress = None;
        self.window = None;
        self.gfx = None;
        self.ui = None;
    }

    /// 更新进度窗口状态
    pub fn update(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_config: &lumino_core::storage::config::UiConfig,
    ) {
        if self.progress.is_some() && self.window.is_none() {
            self.create_window(event_loop, ui_config);
        } else if self.progress.is_none() && self.window.is_some() {
            self.close();
        }
    }

    /// 创建进度窗口
    fn create_window(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_config: &lumino_core::storage::config::UiConfig,
    ) {
        let attributes = WindowAttributes::default()
            .with_inner_size(dpi::LogicalSize {
                width: PROGRESS_WINDOW_WIDTH,
                height: PROGRESS_WINDOW_HEIGHT,
            })
            .with_title("MIDI 处理进度")
            .with_decorations(true)
            .with_visible(true);

        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("创建进度窗口失败: {}", e);
                return;
            }
        };

        let physical_size = window.inner_size();

        let gfx = match futures::executor::block_on(lumino_gfx::Context::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
        )) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("初始化进度窗口图形上下文失败: {}", e);
                return;
            }
        };

        let ui = lumino_ui::Host::new(
            window.clone(),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
            true,
        );

        self.window = Some(window);
        self.gfx = Some(gfx);
        self.ui = Some(ui);
    }

    /// 处理进度消息
    pub fn process_messages(
        &mut self,
        main_ui: &mut lumino_ui::Host,
        main_window: &winit::window::Window,
    ) {
        while let Ok((msg, progress)) = self.rx.try_recv() {
            self.handle_message(msg, progress, main_ui, main_window);
        }
    }

    /// 处理单个进度消息
    fn handle_message(
        &mut self,
        msg: String,
        progress: f64,
        main_ui: &mut lumino_ui::Host,
        main_window: &winit::window::Window,
    ) {
        // 进度完成（>= 1.0）时，关闭进度窗口
        if progress >= 1.0 {
            // 先显示完成消息，然后关闭窗口
            main_ui.update_progress(Some((msg.clone(), progress)));
            if let Some(ref mut ui) = self.ui {
                ui.update_progress(Some((msg, progress)));
            }
            // 请求重绘以显示最终状态
            main_window.request_redraw();
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
            // 关闭进度窗口
            self.progress = None;
            return;
        }

        self.progress = Some((msg.clone(), progress));
        main_ui.update_progress(Some((msg.clone(), progress)));
        if let Some(ref mut ui) = self.ui {
            ui.update_progress(Some((msg, progress)));
        }
        main_window.request_redraw();
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// 请求进度窗口重绘
    pub fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}
