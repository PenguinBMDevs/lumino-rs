use std::collections::HashMap;
use std::sync::Arc;
use winit::{
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};

/// 对话框类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogType {
    CustomPrecision,
    Collaboration,
    LoadConfirm,
}

/// 对话框结果
#[derive(Debug, Clone)]
pub enum DialogResult {
    CustomPrecision {
        numerator: String,
        denominator: String,
    },
    LoadConfirm {
        skip_memory_manager: bool,
    },
}

/// 对话框窗口
/// 每个对话框都是独立的窗口，有自己的渲染上下文
pub struct DialogWindow {
    window: Arc<Window>,
    gfx: Option<lumino_gfx::Context>,
    ui: Option<lumino_ui::Host>,
    dialog_type: DialogType,
    should_close: bool,
    result_data: Option<DialogResult>,
}

impl DialogWindow {
    pub fn new(
        event_loop: &ActiveEventLoop,
        dialog_type: DialogType,
        _parent_window: Option<&Arc<Window>>,
    ) -> Result<Self, String> {
        let (width, height, title) = match dialog_type {
            DialogType::CustomPrecision => (480.0, 180.0, "自定义贴合"),
            DialogType::Collaboration => (420.0, 320.0, "多人协作"),
            DialogType::LoadConfirm => (420.0, 260.0, "加载大文件"),
        };

        let attributes = WindowAttributes::default()
            .with_inner_size(LogicalSize { width, height })
            .with_title(title)
            .with_visible(false)
            .with_decorations(true)
            .with_resizable(false);

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|e| format!("创建对话框窗口失败: {e}"))?,
        );

        Ok(Self {
            window,
            gfx: None,
            ui: None,
            dialog_type,
            should_close: false,
            result_data: None,
        })
    }

    /// 初始化对话框，并同步协作状态（从主窗口）
    pub fn initialize_with_collaboration_state(
        &mut self,
        ui_config: &lumino_core::storage::config::UiConfig,
        main_ui: &lumino_ui::Host,
    ) -> Result<(), String> {
        let physical_size = self.window.inner_size();

        // 检查窗口大小是否为零
        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零，无法初始化".to_string());
        }

        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            self.window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        let mut ui = lumino_ui::Host::new_dialog(
            self.window.clone(),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
        );

        // 根据对话框类型初始化不同的UI内容
        match self.dialog_type {
            DialogType::CustomPrecision => {
                ui.set_custom_precision_dialog_open(true);
            }
            DialogType::Collaboration => {
                ui.sync_collaboration_state_from(main_ui);
            }
            DialogType::LoadConfirm => {
                // LoadConfirm 用默认状态，不需要额外初始化
            }
        }

        self.window.set_visible(true);

        self.gfx = Some(gfx);
        self.ui = Some(ui);

        Ok(())
    }

    /// 初始化加载确认对话框（带文件信息）
    pub fn initialize_load_confirm(
        &mut self,
        ui_config: &lumino_core::storage::config::UiConfig,
        file_path: &str,
        size_mb: f64,
    ) -> Result<(), String> {
        let physical_size = self.window.inner_size();
        if physical_size.width == 0 || physical_size.height == 0 {
            return Err("窗口大小为零".to_string());
        }

        let gfx = futures::executor::block_on(lumino_gfx::Context::new(
            self.window.clone(),
            physical_size.width,
            physical_size.height,
        ))
        .map_err(|e| format!("初始化图形上下文失败: {e}"))?;

        let mut ui = lumino_ui::Host::new_dialog(
            self.window.clone(),
            physical_size.width,
            physical_size.height,
            ui_config,
            &gfx,
        );

        // 设置加载确认对话框状态
        ui.set_load_confirm_dialog(file_path, size_mb);

        self.window.set_visible(true);
        self.gfx = Some(gfx);
        self.ui = Some(ui);

        Ok(())
    }

    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    pub fn handle_event(&mut self, event: WindowEvent) {
        // 先处理需要特殊处理的事件
        match &event {
            WindowEvent::Resized(size) => {
                // 避免零大小窗口导致的 wgpu 错误
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
                // 更新光标位置，使 UI 能正确响应鼠标事件
                if let Some(ui) = self.ui.as_mut() {
                    ui.cursor_moved(*position);
                }
            }
            _ => {}
        }

        // 处理事件 - 传递给 UI
        if let Some(ui) = self.ui.as_mut() {
            ui.handle_events(event, winit::keyboard::ModifiersState::default());
        }
    }

    /// 检查并获取对话框结果（需要在 handle_event 之后调用）
    pub fn check_result(&mut self) -> Option<DialogResult> {
        if let Some(ui) = self.ui.as_mut()
            && let Some(result) = ui.take_dialog_result()
        {
            match result {
                lumino_ui::host::DialogResult::CustomPrecision {
                    numerator,
                    denominator,
                } => {
                    self.result_data = Some(DialogResult::CustomPrecision {
                        numerator,
                        denominator,
                    });
                }
                lumino_ui::host::DialogResult::LoadConfirm {
                    skip_memory_manager,
                } => {
                    self.result_data = Some(DialogResult::LoadConfirm {
                        skip_memory_manager,
                    });
                }
            }
            self.should_close = true;
            return self.result_data.take();
        }
        None
    }

    pub fn redraw(&mut self) {
        // 检查窗口大小是否为零，避免 wgpu 错误
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

    pub fn should_close(&self) -> bool {
        self.should_close
    }

    pub fn request_close(&mut self) {
        self.should_close = true;
    }
}

/// 对话框管理器
/// 负责创建、管理和销毁对话框窗口
pub struct DialogManager {
    /// 活跃的对话框窗口
    dialogs: HashMap<WindowId, DialogWindow>,
    /// 等待初始化的对话框配置
    pending_dialogs: Vec<PendingDialog>,
}

/// 等待创建的对话框配置
#[derive(Debug, Clone)]
pub struct PendingDialog {
    pub dialog_type: DialogType,
    /// LoadConfirm 的 pending path
    pub pending_path: Option<String>,
    pub pending_size_mb: Option<f64>,
}

impl DialogManager {
    pub fn new() -> Self {
        Self {
            dialogs: HashMap::new(),
            pending_dialogs: Vec::new(),
        }
    }

    /// 请求打开一个对话框
    pub fn open_dialog(&mut self, dialog_type: DialogType) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type,
            pending_path: None,
            pending_size_mb: None,
        });
    }

    pub fn open_load_confirm(&mut self, path: String, size_mb: f64) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::LoadConfirm,
            pending_path: Some(path),
            pending_size_mb: Some(size_mb),
        });
    }

    /// 初始化等待中的对话框，并同步主窗口的协作状态
    pub fn initialize_pending_with_collaboration_state(
        &mut self,
        event_loop: &ActiveEventLoop,
        parent_window: &Arc<winit::window::Window>,
        ui_config: &lumino_core::storage::config::UiConfig,
        main_ui: &lumino_ui::Host,
    ) {
        while let Some(pending) = self.pending_dialogs.pop() {
            let mut dialog =
                match DialogWindow::new(event_loop, pending.dialog_type, Some(parent_window)) {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!("创建对话框失败: {}", e);
                        continue;
                    }
                };

            let window_id = dialog.window_id();

            // 初始化对话框
            match pending.dialog_type {
                DialogType::LoadConfirm => {
                    let path = pending.pending_path.unwrap_or_default();
                    let size_mb = pending.pending_size_mb.unwrap_or(0.0);
                    if let Err(e) = dialog.initialize_load_confirm(ui_config, &path, size_mb) {
                        tracing::error!("初始化加载确认对话框失败: {}", e);
                        continue;
                    }
                }
                _ => {
                    if let Err(e) = dialog.initialize_with_collaboration_state(ui_config, main_ui) {
                        tracing::error!("初始化对话框失败: {}", e);
                        continue;
                    }
                }
            }

            tracing::info!("对话框已创建: {:?}", pending.dialog_type);
            self.dialogs.insert(window_id, dialog);
        }
    }

    /// 检查是否是对话框窗口
    pub fn is_dialog_window(&self, window_id: WindowId) -> bool {
        self.dialogs.contains_key(&window_id)
    }

    /// 获取对话框的可变引用
    pub fn get_dialog_mut(&mut self, window_id: WindowId) -> Option<&mut DialogWindow> {
        self.dialogs.get_mut(&window_id)
    }

    /// 关闭对话框
    pub fn close_dialog(&mut self, window_id: WindowId) {
        if self.dialogs.remove(&window_id).is_some() {
            tracing::info!("对话框已关闭: {:?}", window_id);
        }
    }

    /// 设置对话框为待关闭状态
    pub fn mark_dialog_for_close(&mut self, dialog_type: DialogType) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == dialog_type {
                dialog.request_close();
            }
        }
    }

    /// 更新所有对话框（渲染等）
    pub fn update(&mut self) {
        for dialog in self.dialogs.values_mut() {
            if !dialog.should_close() {
                dialog.redraw();
            }
        }
    }
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}
