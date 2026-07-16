//! 对话框管理器
//!
//! 负责创建、管理和销毁对话框窗口。

use std::collections::HashMap;
use std::sync::Arc;

use lumino_core::storage::config::UiConfig;
use lumino_ui::state::root_state::DialogType;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::window::DialogWindow;

/// 等待创建的对话框配置
#[derive(Debug, Clone)]
pub struct PendingDialog {
    pub dialog_type: DialogType,
    /// LoadConfirm 的 pending path
    pub pending_path: Option<String>,
    pub pending_size_mb: Option<f64>,
    /// ProjectSettings 的窗口标题
    pub pending_title: Option<String>,
}

/// 对话框管理器
///
/// 负责创建、管理和销毁对话框窗口。
pub struct DialogManager {
    /// 活跃的对话框窗口
    dialogs: HashMap<WindowId, DialogWindow>,
    /// 等待初始化的对话框配置
    pending_dialogs: Vec<PendingDialog>,
}

impl DialogManager {
    /// 创建新的对话框管理器
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
            pending_title: None,
        });
    }

    /// 请求打开工程设置对话框（带自定义标题）
    pub fn open_project_settings(&mut self, title: String) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::ProjectSettings,
            pending_path: None,
            pending_size_mb: None,
            pending_title: Some(title),
        });
    }

    /// 请求打开加载确认对话框
    pub fn open_load_confirm(&mut self, path: String, size_mb: f64) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::LoadConfirm,
            pending_path: Some(path),
            pending_size_mb: Some(size_mb),
            pending_title: None,
        });
    }

    /// 初始化等待中的对话框，并同步主窗口的协作状态
    pub fn initialize_pending_with_collaboration_state(
        &mut self,
        event_loop: &ActiveEventLoop,
        parent_window: &Arc<Window>,
        ui_config: &UiConfig,
        main_ui: &lumino_ui::Host,
    ) {
        while let Some(pending) = self.pending_dialogs.pop() {
            let mut dialog = match DialogWindow::new(
                event_loop,
                pending.dialog_type,
                Some(parent_window),
                ui_config,
            ) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("创建对话框失败: {}", e);
                    continue;
                }
            };

            let window_id = dialog.window_id();

            match pending.dialog_type {
                DialogType::LoadConfirm => {
                    let path = pending.pending_path.unwrap_or_default();
                    let size_mb = pending.pending_size_mb.unwrap_or(0.0);
                    if let Err(e) = dialog.initialize_load_confirm(ui_config, &path, size_mb) {
                        tracing::error!("初始化加载确认对话框失败: {}", e);
                        continue;
                    }
                }
                DialogType::ProjectSettings => {
                    if let Some(title) = pending.pending_title {
                        dialog.set_window_title(&title);
                    }
                    if let Err(e) = dialog.initialize_project_settings(ui_config, main_ui) {
                        tracing::error!("初始化工程设置对话框失败: {}", e);
                        continue;
                    }
                }
                DialogType::Settings => {
                    if let Err(e) = dialog.initialize_with_collaboration_state(ui_config, main_ui) {
                        tracing::error!("初始化设置对话框失败: {}", e);
                        continue;
                    }
                }
                DialogType::ExportProgress => {
                    if let Err(e) = dialog.initialize_export_progress(ui_config) {
                        tracing::error!("初始化导出进度对话框失败: {}", e);
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

    /// 检查指定类型的对话框是否存在
    pub fn has_dialog_type(&self, dialog_type: DialogType) -> bool {
        self.dialogs.values().any(|d| d.dialog_type == dialog_type)
    }

    /// 转发视频导出进度到 VideoExport 对话框
    pub fn forward_video_export_progress(
        &mut self,
        message: String,
        progress: f64,
        total_frames: u64,
        render_fps: f64,
    ) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.update_video_export_progress(
                    message.clone(),
                    progress,
                    total_frames,
                    render_fps,
                );
            }
        }
    }

    /// 转发视频导出预览帧到 VideoExport 对话框
    pub fn forward_video_export_preview_frame(&mut self, data: Vec<u8>, w: u32, h: u32) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.update_video_export_preview_frame(data.clone(), w, h);
            }
        }
    }

    /// 转发视频导出完成到 VideoExport 对话框
    pub fn forward_video_export_completed(&mut self, elapsed_secs: f64) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_video_export_completed(elapsed_secs);
            }
        }
    }

    /// 转发视频导出失败到 VideoExport 对话框
    pub fn forward_video_export_failed(&mut self, error: String) {
        for dialog in self.dialogs.values_mut() {
            if dialog.dialog_type == DialogType::VideoExport
                && let Some(ui) = dialog.ui_mut()
            {
                ui.set_video_export_failed(error.clone());
            }
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
    ///
    /// 内存监控对话框使用 `redraw_force` 确保每次刷新重建 UI 界面（`view()` 重新捕获 Snapshot），
    /// 其他对话框走正常 `redraw`（若 `ui_dirty == false` 则进入 iced 缓存早退路径，无额外开销）。
    pub fn update(&mut self) {
        for dialog in self.dialogs.values_mut() {
            if dialog.should_close() {
                continue;
            }
            match dialog.dialog_type {
                DialogType::MemoryMonitor => {
                    dialog.redraw_force();
                }
                _ => {
                    dialog.redraw();
                }
            }
        }
    }
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}
