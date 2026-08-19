//! 云传输（上传/下载）进度悬浮窗口管理器
//!
//! 模仿 MIDI 加载进度窗口（`ProgressManager`）实现：
//! - 独立小窗口 + 进度条 UI（复用 `Root::new_progress` 进度视图）
//! - 通过 mpsc 通道接收 `(消息, 进度)` 更新，进度 >= 1.0 时自动关闭
//!
//! 与 `ProgressManager` 的差异：
//! - **覆盖型悬浮定位**：窗口创建时定位到锚点窗口（云浏览对话框）中心，
//!   视觉上覆盖在对应的下载/上传窗口之上；无云浏览对话框时回退到主窗口中心
//! - 云上传/下载为阻塞调用无精确进度回调，采用阶段进度（0.1 开始 → 1.0 结束）

use std::sync::Arc;
use tokio::sync::mpsc;
use winit::{
    dpi::{LogicalSize, PhysicalPosition, Position},
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

/// 云进度悬浮窗口尺寸（比 MIDI 进度窗更小巧，悬浮感更强）
pub const CLOUD_PROGRESS_WIDTH: u32 = 400;
pub const CLOUD_PROGRESS_HEIGHT: u32 = 160;

/// 云传输进度悬浮窗口管理器
pub struct CloudProgressManager {
    /// 进度接收器
    rx: mpsc::UnboundedReceiver<(String, f64)>,
    /// 当前进度
    progress: Option<(String, f64)>,
    /// 悬浮窗口
    window: Option<Arc<Window>>,
    /// 悬浮窗口图形上下文
    gfx: Option<lumino_gfx::Context>,
    /// 悬浮窗口 UI
    ui: Option<lumino_ui::Host>,
    /// 修饰键状态
    modifiers: winit::keyboard::ModifiersState,
}

impl CloudProgressManager {
    /// 创建管理器（返回发送端供后台线程推送进度）
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

    /// 检查是否是云进度悬浮窗口的 ID
    pub fn is_cloud_progress_window(&self, window_id: winit::window::WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == window_id)
    }

    /// 处理悬浮窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                self.handle_redraw(&window);
                // 应用自制标题栏产生的窗口动作（关闭 / 最小化 / 拖动）
                self.apply_window_actions();
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
    fn handle_redraw(&mut self, window: &Arc<Window>) {
        if let Some(ref mut ui) = self.ui
            && let Some(ref gfx) = self.gfx
            && gfx
                .with_frame(|frame, view| ui.redraw_requested(frame, view, gfx))
                .is_err()
        {
            window.request_redraw();
        }
    }

    /// 应用自制标题栏产生的窗口动作（关闭 / 最小化 / 拖动）
    fn apply_window_actions(&mut self) {
        let (action, drag) = match self.ui.as_mut() {
            Some(ui) => (ui.take_window_action(), ui.take_drag()),
            None => (None, false),
        };

        if let Some(action) = action {
            match action {
                lumino_ui::window::TrafficAction::Close => self.close(),
                lumino_ui::window::TrafficAction::Minimize => {
                    if let Some(window) = self.window.as_ref() {
                        window.set_minimized(true);
                    }
                }
                lumino_ui::window::TrafficAction::ToggleMaximize => {
                    // 悬浮窗不可缩放，无需响应最大化
                    if let Some(window) = self.window.as_ref()
                        && window.is_resizable()
                    {
                        let is_max = window.is_maximized();
                        window.set_maximized(!is_max);
                    }
                }
            }
        }

        if drag
            && let Some(window) = self.window.clone()
            && let Err(e) = window.drag_window()
        {
            tracing::warn!("拖动云进度悬浮窗失败: {}", e);
            if let Some(ui) = self.ui.as_mut() {
                ui.release_left_mouse_button();
            }
        }
    }

    /// 处理大小改变
    fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>, window: &Arc<Window>) {
        if let Some(ref mut ui) = self.ui {
            ui.resize(size.width, size.height);
        }
        if let Some(ref mut gfx) = self.gfx {
            gfx.resize(size.width, size.height);
        }
        window.request_redraw();
    }

    /// 关闭悬浮窗口
    pub fn close(&mut self) {
        self.progress = None;
        self.window = None;
        self.gfx = None;
        self.ui = None;
    }

    /// 更新悬浮窗口状态（about_to_wait 中调用）
    ///
    /// `anchor` 为覆盖定位的锚点窗口：云浏览对话框（有则用）或主窗口。
    pub fn update(
        &mut self,
        event_loop: &ActiveEventLoop,
        ui_config: &lumino_core::storage::config::UiConfig,
        anchor: Option<&Window>,
    ) {
        if self.progress.is_some() && self.window.is_none() {
            self.create_window(event_loop, ui_config, anchor);
        } else if self.progress.is_none() && self.window.is_some() {
            self.close();
        }
    }

    /// 创建悬浮窗口：尺寸小巧，定位覆盖在锚点窗口中心
    fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        ui_config: &lumino_core::storage::config::UiConfig,
        anchor: Option<&Window>,
    ) {
        let mut attributes = WindowAttributes::default()
            .with_inner_size(LogicalSize {
                width: CLOUD_PROGRESS_WIDTH,
                height: CLOUD_PROGRESS_HEIGHT,
            })
            .with_title("云存储传输")
            .with_visible(true);

        // 悬浮窗跟随主窗口的标题栏配置。
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::WindowAttributesExtWindows;
            attributes = if ui_config.use_native_titlebar {
                attributes.with_decorations(true)
            } else {
                attributes
                    .with_decorations(false)
                    .with_undecorated_shadow(true)
            };
        }
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            if !ui_config.use_native_titlebar {
                attributes = attributes
                    .with_titlebar_transparent(true)
                    .with_fullsize_content_view(true);
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            attributes = attributes.with_decorations(ui_config.use_native_titlebar);
        }

        // 覆盖型定位：窗口中心对齐锚点窗口中心
        if let Some(anchor) = anchor
            && let Ok(pos) = anchor.outer_position()
        {
            let size = anchor.outer_size();
            let x = pos.x + size.width as i32 / 2 - CLOUD_PROGRESS_WIDTH as i32 / 2;
            let y = pos.y + size.height as i32 / 2 - CLOUD_PROGRESS_HEIGHT as i32 / 2;
            attributes = attributes.with_position(Position::Physical(PhysicalPosition { x, y }));
        }

        let window = match event_loop.create_window(attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("创建云进度悬浮窗失败: {}", e);
                return;
            }
        };

        let physical_size = window.inner_size();

        let gfx = match lumino_gfx::Context::new_blocking(
            Arc::clone(&window),
            physical_size.width,
            physical_size.height,
        ) {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("初始化云进度悬浮窗图形上下文失败: {}", e);
                return;
            }
        };

        let ui = lumino_ui::Host::new(
            Arc::clone(&window),
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

    /// 处理进度消息（about_to_wait 中调用）
    pub fn process_messages(&mut self) {
        while let Ok((msg, progress)) = self.rx.try_recv() {
            self.handle_message(msg, progress);
        }
    }

    /// 处理单条进度消息：进度 >= 1.0 时关闭悬浮窗
    fn handle_message(&mut self, msg: String, progress: f64) {
        if progress >= 1.0 {
            // 先刷新一次最终状态，再关闭窗口
            if let Some(ref mut ui) = self.ui {
                ui.update_progress(Some((msg, progress)));
            }
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
            self.progress = None;
            return;
        }

        self.progress = Some((msg.clone(), progress));
        if let Some(ref mut ui) = self.ui {
            ui.update_progress(Some((msg, progress)));
        }
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }
}
