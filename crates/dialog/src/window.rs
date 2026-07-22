//! 对话框窗口
//!
//! 每个对话框都是独立的 winit 窗口，拥有自己的渲染上下文和 UI Host。

use std::sync::Arc;

use lumino_core::storage::config::UiConfig;
use lumino_ui::host::DialogResult;
use lumino_ui::state::root_state::DialogType;
use lumino_ui::window::TrafficAction;
use winit::{
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};

/// 对话框窗口
///
/// 每个对话框都是独立的窗口，有自己的渲染上下文和 UI 生命周期。
pub struct DialogWindow {
    window: Arc<Window>,
    gfx: Option<lumino_gfx::Context>,
    ui: Option<lumino_ui::Host>,
    pub(crate) dialog_type: DialogType,
    should_close: bool,
    result_data: Option<DialogResult>,
}

impl DialogWindow {
    /// 创建新对话框窗口（仅创建 winit 窗口，不初始化 GFX/UI）
    ///
    /// 将原本一次性的阻塞初始化拆分为多阶段：
    /// 窗口创建 → GFX 初始化 → UI 初始化 → 显示窗口。
    /// 这样可以避免在 `about_to_wait` 单帧中阻塞事件循环 900ms+。
    pub fn new(
        event_loop: &ActiveEventLoop,
        dialog_type: DialogType,
        _parent_window: Option<&Arc<Window>>,
        ui_config: &UiConfig,
    ) -> Result<Self, String> {
        puffin::profile_scope!("dialog_init_phase_window");
        let (width, height, title, resizable) = match dialog_type {
            DialogType::None => unreachable!("不会创建 None 类型的对话框"),
            DialogType::CustomPrecision => (480.0, 180.0, "自定义贴合", false),
            DialogType::Collaboration => (420.0, 320.0, "多人协作", false),
            DialogType::LoadConfirm => (420.0, 260.0, "加载大文件", false),
            DialogType::ProjectSettings => (450.0, 480.0, "工程设置", true),
            DialogType::Settings => (700.0, 500.0, "设置", true),
            DialogType::SpeedChange => (400.0, 250.0, "变速", false),
            DialogType::BatchEdit => (420.0, 640.0, "批量编辑", true),
            DialogType::ExportProgress => (400.0, 200.0, "音频导出", false),
            DialogType::VideoExport => (520.0, 560.0, "视频导出", false),
            DialogType::MemoryMonitor => (300.0, 440.0, "内存占用详情", false),
        };

        let mut attributes = WindowAttributes::default()
            .with_inner_size(LogicalSize { width, height })
            .with_title(title)
            .with_visible(false)
            .with_resizable(resizable);

        // 弹窗跟随主窗口的标题栏配置；系统模式下不绘制自制标题栏。
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

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建对话框窗口失败: {e}"))?,
        );

        #[cfg(target_os = "windows")]
        if resizable && !ui_config.use_native_titlebar {
            crate::platform::windows::setup_resize_border(&window)?;
        }

        Ok(Self {
            window,
            gfx: None,
            ui: None,
            dialog_type,
            should_close: false,
            result_data: None,
        })
    }

    /// 获取 GFX 上下文的引用（用于判断当前处于哪个初始化阶段）
    pub(crate) fn gfx_ref(&self) -> Option<&lumino_gfx::Context> {
        self.gfx.as_ref()
    }

    /// 第二阶段：为已有窗口创建 GFX 上下文
    pub fn initialize_gfx(&mut self) -> Result<(), String> {
        puffin::profile_scope!("dialog_init_phase_gfx");
        let physical_size = self.window.inner_size();

        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = lumino_gfx::Context::new_blocking(
            self.window.clone(),
            physical_size.width,
            physical_size.height,
        )
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        self.gfx = Some(gfx);
        Ok(())
    }

    /// 第三阶段：初始化 UI 并显示窗口
    ///
    /// 假设 GFX 已通过 `initialize_gfx` 初始化完成。本方法集中处理所有
    /// 对话框类型的 UI 创建与状态同步，避免为每种类型单独写一个初始化函数。
    pub fn initialize_ui(
        &mut self,
        ui_config: &UiConfig,
        main_ui: &lumino_ui::Host,
        pending_path: Option<&str>,
        size_mb: f64,
        pending_title: Option<&str>,
    ) -> Result<(), String> {
        puffin::profile_scope!("dialog_init_phase_ui");
        let physical_size = self.window.inner_size();

        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = self.gfx.as_ref().ok_or("GFX 未初始化")?;

        if let Some(title) = pending_title {
            self.window.set_title(title);
        }

        let mut ui = match self.dialog_type {
            DialogType::Settings => lumino_ui::Host::new_settings_dialog(
                self.window.clone(),
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
            ),
            _ => lumino_ui::Host::new_dialog(
                self.window.clone(),
                physical_size.width,
                physical_size.height,
                ui_config,
                gfx,
                self.dialog_type,
            ),
        };

        match self.dialog_type {
            DialogType::None => {}
            DialogType::CustomPrecision => {
                ui.set_custom_precision_dialog_open(true);
            }
            DialogType::Collaboration => {
                ui.sync_collaboration_state_from(main_ui);
            }
            DialogType::LoadConfirm => {
                let path = pending_path.unwrap_or_default();
                ui.set_load_confirm_dialog(path, size_mb);
            }
            DialogType::ProjectSettings => {
                ui.set_project_settings_dialog_open(true);
                let (title, tempo, copyright, created_display, editing_time) =
                    main_ui.get_project_settings_data();
                ui.set_project_settings_data(
                    title,
                    tempo,
                    copyright,
                    created_display,
                    editing_time,
                );
            }
            DialogType::Settings => {
                ui.set_settings_dialog_open(true);
            }
            DialogType::SpeedChange => {
                ui.set_speed_change_dialog_open(true);
            }
            DialogType::BatchEdit => {
                ui.set_batch_edit_dialog_open(true);
            }
            DialogType::ExportProgress => {
                ui.set_export_progress_dialog_open(true);
            }
            DialogType::VideoExport => {
                ui.update_video_export_progress("正在初始化...".to_string(), 0.0, 0, 0.0);
            }
            DialogType::MemoryMonitor => {
                ui.set_memory_monitor_dialog_open(true);
            }
        }

        self.window.set_visible(true);
        self.ui = Some(ui);

        Ok(())
    }

    /// 获取窗口 ID
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// 设置窗口标题
    pub fn set_window_title(&self, title: &str) {
        self.window.set_title(title);
    }

    /// 请求窗口重绘
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// 处理窗口事件
    pub fn handle_event(&mut self, event: WindowEvent) {
        match &event {
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) {
                    ui.resize(size.width, size.height);
                    gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::CloseRequested => {
                self.should_close = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ui) = self.ui.as_mut() {
                    ui.cursor_moved(*position);
                }
            }
            _ => {}
        }

        if let Some(ui) = self.ui.as_mut() {
            ui.handle_events(event, winit::keyboard::ModifiersState::default());
        }
    }

    /// 应用自制标题栏产生的窗口动作（关闭 / 最小化 / 拖动）
    ///
    /// 无边框模式下，系统标题栏已移除，窗口控制完全由 iced 渲染的自制
    /// 标题栏（`Titlebar`）承担：红绿灯按钮与拖动区通过 `window::Event`
    /// 消息，由 `Host::process_message` 写入 `pending_window_action` /
    /// `pending_drag`。这些动作必须在 `redraw()`（事件队列已被消费）之后
    /// 取出并应用到真实的 winit 窗口。
    ///
    /// 与 `WindowManager::handle_window_actions` 的关键差异：对话框的 `Close`
    /// **不会退出事件循环**，仅标记 `should_close`，由 runner 负责关闭该对话框。
    pub fn apply_window_actions(&mut self) {
        let (action, drag) = match self.ui.as_mut() {
            Some(ui) => (ui.take_window_action(), ui.take_drag()),
            None => (None, false),
        };

        if let Some(action) = action {
            match action {
                TrafficAction::Close => {
                    // 关闭对话框（不退出整个事件循环）
                    self.should_close = true;
                }
                TrafficAction::Minimize => {
                    self.window.set_minimized(true);
                }
                TrafficAction::ToggleMaximize => {
                    // 不可缩放的对话框无需响应最大化
                    if self.window.is_resizable() {
                        let is_max = self.window.is_maximized();
                        self.window.set_maximized(!is_max);
                    }
                }
            }
        }

        if drag && let Err(e) = self.window.drag_window() {
            tracing::warn!("拖动对话框窗口失败: {}", e);
            if let Some(ui) = self.ui.as_mut() {
                ui.release_left_mouse_button();
            }
        }
    }

    /// 检查并获取对话框结果（需在 handle_event 之后调用）
    pub fn check_result(&mut self) -> Option<DialogResult> {
        if let Some(ui) = self.ui.as_mut()
            && let Some(result) = ui.take_dialog_result()
        {
            self.result_data = Some(result);
            self.should_close = true;
            return self.result_data.take();
        }
        None
    }

    /// 请求重绘
    pub fn redraw(&mut self) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        if let (Some(gfx), Some(ui)) = (self.gfx.as_mut(), self.ui.as_mut()) {
            match gfx.with_frame(|frame, view| ui.redraw_requested(frame, view, gfx)) {
                Ok(_) => {}
                Err(_) => {
                    self.window.request_redraw();
                }
            }
        }
    }

    /// 强制重绘：先标记 UI 脏确保 `view()` 重新构建，再执行正常 `redraw`。
    ///
    /// 用于内存监控等需要每帧重新捕获快照的对话框，避免 `render_iced_ui`
    /// 因 `ui_dirty == false` 进入「仅 present 缓存」的早退路径而跳过
    /// `UserInterface::build`（从而跳过 `view()` 中重新捕获 Snapshot）。
    pub fn redraw_force(&mut self) {
        if let Some(ui) = self.ui.as_mut() {
            ui.mark_dirty();
        }
        self.redraw();
    }

    /// 是否应关闭
    pub fn should_close(&self) -> bool {
        self.should_close
    }

    /// 请求关闭
    pub fn request_close(&mut self) {
        self.should_close = true;
    }

    /// 获取对话框 UI 的可变引用
    pub fn ui_mut(&mut self) -> Option<&mut lumino_ui::Host> {
        self.ui.as_mut()
    }
}
