//! 设备检查警告窗：独立 winit 窗口 + gfx Context + iced Host 管理器。
//!
//! 生命周期仿 `ProgressManager`：窗口创建 → 事件转发 → 用户操作回传 → 关闭。
//! 警告窗只在"首启/未抑制 + 检测失败"时短暂存在；用户选择由
//! [`lumino_ui::Host::take_device_warning_choice`] 取出，交由 Runner 决策。

use std::sync::Arc;

use winit::{dpi, event::WindowEvent, window::WindowAttributes};

/// 警告窗宽度（逻辑像素）
pub(crate) const DEVICE_WARNING_WINDOW_WIDTH: u32 = 560;
/// 警告窗高度（逻辑像素）
pub(crate) const DEVICE_WARNING_WINDOW_HEIGHT: u32 = 200;

/// 设备检查警告窗管理器
pub(crate) struct DeviceWarningWindow {
    window: Option<Arc<winit::window::Window>>,
    gfx: Option<lumino_gfx::Context>,
    ui: Option<lumino_ui::Host>,
    modifiers: winit::keyboard::ModifiersState,
    /// 用户直接关闭窗口（[X] / Alt+F4）→ 视为「确认并关闭」
    close_requested: bool,
}

impl DeviceWarningWindow {
    /// 创建警告窗。任何一步失败都返回 `Err`，由调用方走原生提示框兜底。
    pub(crate) fn create(
        event_loop: &winit::event_loop::ActiveEventLoop,
        ui_config: &lumino_core::storage::config::UiConfig,
        detail: &str,
    ) -> Result<Self, String> {
        let mut attributes = WindowAttributes::default()
            .with_inner_size(dpi::LogicalSize {
                width: DEVICE_WARNING_WINDOW_WIDTH,
                height: DEVICE_WARNING_WINDOW_HEIGHT,
            })
            .with_title("GPU 兼容性检查")
            .with_resizable(false)
            .with_visible(true);

        // 窗口装饰跟随主窗口配置（与进度窗口一致）
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

        let window = event_loop
            .create_window(attributes)
            .map_err(|e| format!("创建警告窗口失败: {e}"))?;
        let window = Arc::new(window);
        let size = window.inner_size();

        let gfx = lumino_gfx::Context::new_blocking(Arc::clone(&window), size.width, size.height)
            .map_err(|e| format!("初始化警告窗图形上下文失败: {e}"))?;

        let ui = lumino_ui::Host::new_device_warning(
            Arc::clone(&window),
            size.width,
            size.height,
            ui_config,
            &gfx,
            detail,
        );
        window.request_redraw();

        Ok(Self {
            window: Some(window),
            gfx: Some(gfx),
            ui: Some(ui),
            modifiers: winit::keyboard::ModifiersState::default(),
            close_requested: false,
        })
    }

    /// 是否为该警告窗的窗口 ID
    pub(crate) fn is_window(&self, window_id: winit::window::WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == window_id)
    }

    /// 处理窗口事件（重绘 / 光标 / 尺寸 / 关闭）
    pub(crate) fn handle_event(&mut self, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                if let Some(ref mut ui) = self.ui
                    && let Some(ref gfx) = self.gfx
                    && gfx
                        .with_frame(|frame, view| ui.redraw_requested(frame, view, gfx))
                        .is_err()
                {
                    window.request_redraw();
                }
                // 应用自制标题栏产生的窗口动作（关闭/最小化/拖动）
                // 必须在 redraw（事件队列已消费）之后调用
                self.apply_window_actions();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref mut ui) = self.ui {
                    ui.cursor_moved(position);
                }
                window.request_redraw();
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut ui) = self.ui {
                    ui.resize(size.width, size.height);
                }
                if let Some(ref mut gfx) = self.gfx {
                    gfx.resize(size.width, size.height);
                }
                window.request_redraw();
            }
            WindowEvent::CloseRequested => {
                self.close_requested = true;
            }
            other => {
                if let Some(ref mut ui) = self.ui {
                    ui.handle_events(other, self.modifiers);
                }
                window.request_redraw();
            }
        }
    }

    /// 应用自制标题栏产生的窗口动作（关闭 / 最小化 / 拖动）
    ///
    /// 无边框模式下窗口控制由 iced 渲染的自制标题栏承担，动作需在
    /// `redraw_requested`（事件队列已消费）之后取出并应用到真实窗口；
    /// 「关闭」视为「确认并关闭」（`close_requested` 由 `take_action` 转为 Quit）。
    fn apply_window_actions(&mut self) {
        let (action, drag) = match self.ui.as_mut() {
            Some(ui) => (ui.take_window_action(), ui.take_drag()),
            None => (None, false),
        };

        if let Some(action) = action {
            match action {
                lumino_ui::window::TrafficAction::Close => {
                    self.close_requested = true;
                }
                lumino_ui::window::TrafficAction::Minimize => {
                    if let Some(window) = self.window.as_ref() {
                        window.set_minimized(true);
                    }
                }
                lumino_ui::window::TrafficAction::ToggleMaximize => {
                    // 警告窗不可缩放，无需响应最大化
                }
            }
        }

        if drag
            && let Some(window) = self.window.clone()
            && let Err(e) = window.drag_window()
        {
            tracing::warn!("拖动警告窗失败: {e}");
            if let Some(ui) = self.ui.as_mut() {
                ui.release_left_mouse_button();
            }
        }
    }

    /// 取走用户操作（UI 按钮优先；其次窗口关闭视为「确认并关闭」）
    pub(crate) fn take_action(&mut self) -> Option<lumino_ui::window::DeviceWarningAction> {
        if let Some(ref mut ui) = self.ui
            && let Some(action) = ui.take_device_warning_choice()
        {
            return Some(action);
        }
        if self.close_requested {
            return Some(lumino_ui::window::DeviceWarningAction::Quit);
        }
        None
    }

    /// 关闭并释放窗口资源
    pub(crate) fn close(&mut self) {
        self.window = None;
        self.gfx = None;
        self.ui = None;
    }
}
