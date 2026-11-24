//! 通用浮窗生命周期基类
//!
//! 将进度窗 / 云进度窗 / 设备警告窗三套重复的窗口创建、事件泵、重绘、resize、
//! 重绑与关闭收敛为单份实现。规格差异全部走 [`OverlaySpec`]，关闭语义走
//! [`ClosePolicy`]。阶段一仅接入进度窗与云进度窗，警告窗在阶段二并入。

use std::sync::Arc;

use winit::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Position},
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowButtons, WindowId},
};

use lumino_core::storage::config::UiConfig;
use lumino_ui::window::TrafficAction;

/// 关闭语义：立即释放，或置标志交调用方决策（阶段二警告窗）。
#[derive(Clone, Copy)]
pub(crate) enum ClosePolicy {
    /// 立即关闭窗口并释放资源
    Immediate,
    /// 置 `close_requested`，由调用方 `take_close_requested` 决策
    #[allow(dead_code)]
    Flag,
}

/// 浮窗规格：三套窗口的全部差异项。
pub(crate) struct OverlaySpec {
    pub(crate) size: LogicalSize<u32>,
    pub(crate) title: String,
    pub(crate) resizable: bool,
    pub(crate) buttons: Option<WindowButtons>,
    pub(crate) close_policy: ClosePolicy,
}

/// 通用浮窗：窗口 + 图形上下文 + Host + egui 输入缓冲。
pub(crate) struct OverlayWindow {
    window: Option<Arc<Window>>,
    gfx: Option<lumino_gfx::Context>,
    ui: Option<lumino_ui::Host>,
    egui_input: lumino_ui::host::OverlayInput,
    modifiers: winit::keyboard::ModifiersState,
    close_policy: ClosePolicy,
    /// 阶段二警告窗用；阶段一无读取方
    #[allow(dead_code)]
    close_requested: bool,
}

impl OverlayWindow {
    /// 按规格创建浮窗；锚点存在时居中覆盖。
    pub(crate) fn create(
        event_loop: &ActiveEventLoop,
        ui_config: &UiConfig,
        spec: &OverlaySpec,
        anchor: Option<&Window>,
    ) -> Result<Self, String> {
        let size = spec.size;
        let mut attributes = WindowAttributes::default()
            .with_inner_size(size)
            .with_title(spec.title.clone())
            .with_visible(true)
            .with_resizable(spec.resizable);
        if let Some(buttons) = spec.buttons {
            attributes = attributes.with_enabled_buttons(buttons);
        }

        // 浮窗跟随主窗口的标题栏配置（三份拷贝原有逻辑，逐字收敛）
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

        // 覆盖型定位：窗口中心对齐锚点窗口中心（云进度窗策略）
        if let Some(anchor) = anchor
            && let Ok(pos) = anchor.outer_position()
        {
            let anchor_size = anchor.outer_size();
            let x = pos.x + anchor_size.width as i32 / 2 - size.width as i32 / 2;
            let y = pos.y + anchor_size.height as i32 / 2 - size.height as i32 / 2;
            attributes = attributes.with_position(Position::Physical(PhysicalPosition { x, y }));
        }

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建浮窗失败: {e}"))?,
        );
        let physical_size = window.inner_size();

        let gfx = lumino_gfx::Context::new_blocking(
            {
                let window = Arc::clone(&window);
                move |instance| instance.create_surface(Arc::clone(&window))
            },
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化浮窗图形上下文失败: {e}"))?;

        let ui = lumino_ui::Host::new(
            lumino_ui::host::host_window(Arc::clone(&window)),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
            true,
        );

        Ok(Self {
            window: Some(window),
            gfx: Some(gfx),
            ui: Some(ui),
            egui_input: lumino_ui::host::OverlayInput::default(),
            modifiers: winit::keyboard::ModifiersState::default(),
            close_policy: spec.close_policy,
            close_requested: false,
        })
    }

    /// 是否为该浮窗的窗口 ID
    pub(crate) fn is_window(&self, window_id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == window_id)
    }

    /// REND-019 PR-C：重连——浮窗存活时重绑到新 device（不存在则 no-op）。
    pub(crate) fn rebind_gpu(&mut self, epoch: lumino_gfx::GpuEpoch, gpu: &lumino_gfx::SharedGpu) {
        if let Some(gfx) = self.gfx.as_mut() {
            gfx.rebind_device(epoch, gpu);
        }
        if let Some(ui) = self.ui.as_mut() {
            ui.rebind_gpu(epoch, gpu);
        }
    }

    /// 处理浮窗事件
    pub(crate) fn handle_event(&mut self, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                self.handle_redraw(&window);
                // 必须在 redraw（事件队列已消费）之后应用标题栏动作
                self.apply_window_actions();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ref mut ui) = self.ui {
                    ui.cursor_moved(position);
                }
            }
            // 纯触屏平台（Android）没有 `CursorMoved`：指针位置必须由 `Touch` 驱动
            WindowEvent::Touch(touch) => {
                if let Some(ref mut ui) = self.ui {
                    ui.cursor_moved(touch.location);
                }
            }
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::Resized(size) => {
                self.handle_resize(size, &window);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // REF-001 G-1：DPI 变化可能不伴随 `Resized`（物理尺寸不变）
                let size = window.inner_size();
                self.handle_resize(size, &window);
            }
            WindowEvent::CloseRequested => match self.close_policy {
                ClosePolicy::Immediate => self.close(),
                ClosePolicy::Flag => self.close_requested = true,
            },
            _ => {
                if let Some(ref mut ui) = self.ui {
                    ui.handle_events(event, self.modifiers, &mut self.egui_input);
                }
            }
        }
    }

    /// 处理重绘
    fn handle_redraw(&mut self, window: &Arc<Window>) {
        let egui_events = self.egui_input.take();
        let egui_focused = self.egui_input.focused();
        if let Some(ref mut ui) = self.ui
            && let Some(ref gfx) = self.gfx
            && let Err(e) = gfx.with_frame(|frame, view| {
                ui.redraw_requested(frame, view, gfx, egui_events, egui_focused)
            })
        {
            // REND-019（PR-A §1.6）：仅可恢复错误安排重绘；
            // Lost/OOM 已置停帧标志（等重连），禁止盲重绘忙循环
            if gfx.handle_frame_error(&e).should_redraw() {
                window.request_redraw();
            }
        }
        if let Some(ui) = self.ui.as_ref() {
            self.egui_input.set_wants_keyboard(ui.egui_wants_keyboard());
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
                TrafficAction::Close => match self.close_policy {
                    ClosePolicy::Immediate => self.close(),
                    ClosePolicy::Flag => self.close_requested = true,
                },
                TrafficAction::Minimize => {
                    if let Some(window) = self.window.as_ref() {
                        window.set_minimized(true);
                    }
                }
                TrafficAction::ToggleMaximize => {
                    // 浮窗通常不可缩放；is_resizable() 兜底保留原行为
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
            tracing::warn!("拖动浮窗失败: {}", e);
            if let Some(ui) = self.ui.as_mut() {
                ui.release_left_mouse_button();
            }
        }
    }

    /// 处理大小改变
    fn handle_resize(&mut self, size: PhysicalSize<u32>, window: &Arc<Window>) {
        if let Some(ref mut ui) = self.ui {
            ui.resize(size.width, size.height);
        }
        if let Some(ref mut gfx) = self.gfx {
            gfx.resize(size.width, size.height);
        }
        window.request_redraw();
    }

    /// 关闭并释放窗口资源
    pub(crate) fn close(&mut self) {
        self.window = None;
        self.gfx = None;
        self.ui = None;
    }

    /// 窗口是否已关闭（薄壳据此同步清理自身状态）
    pub(crate) fn is_closed(&self) -> bool {
        self.window.is_none()
    }

    /// 更新浮窗进度显示
    pub(crate) fn update_progress(&mut self, progress: Option<(String, f64)>) {
        if let Some(ref mut ui) = self.ui {
            ui.update_progress(progress);
        }
    }

    /// 请求浮窗重绘
    pub(crate) fn request_redraw(&self) {
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    /// 取走关闭标志（阶段二警告窗用）
    #[allow(dead_code)]
    pub(crate) fn take_close_requested(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }
}
