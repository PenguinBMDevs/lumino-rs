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
    /// 正在分帧初始化的对话框（已创建窗口，但 GFX/UI 尚未就绪）
    /// 元组保存对应的 PendingDialog，阶段 3 需要其中的配置数据。
    initializing: Vec<(DialogWindow, PendingDialog)>,
}

impl DialogManager {
    /// 创建新的对话框管理器
    pub fn new() -> Self {
        Self {
            dialogs: HashMap::new(),
            pending_dialogs: Vec::new(),
            initializing: Vec::new(),
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

    /// 请求打开"找回删除音轨"对话框
    ///
    /// 实际的条目列表由 Runner 在对话框 UI 就绪后通过
    /// `Host::set_recover_track_dialog_entries` 注入（扫描缓存目录）。
    pub fn open_recover_track(&mut self) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::RecoverTrack,
            pending_path: None,
            pending_size_mb: None,
            pending_title: None,
        });
    }

    /// 分帧处理等待中的对话框初始化。
    ///
    /// 将原本在 `about_to_wait` 中单次同步完成的“创建窗口 + GFX + UI”拆成
    /// 三个阶段，每帧最多推进一个阶段，避免阻塞事件循环 900ms+。
    /// 阶段 1：创建 winit 窗口（hidden）。
    /// 阶段 2：创建 wgpu 图形上下文。
    /// 阶段 3：创建 iced UI Host、同步状态并显示窗口。
    pub fn process_initialization_step(
        &mut self,
        event_loop: &ActiveEventLoop,
        parent_window: &Arc<Window>,
        ui_config: &UiConfig,
        main_ui: &lumino_ui::Host,
    ) {
        // 阶段 1：从 pending 队列取出一个请求，仅创建窗口。
        // 限制同时初始化的窗口数量，避免一帧内堆积多个窗口创建。
        if self.initializing.is_empty()
            && let Some(pending) = self.pending_dialogs.pop()
        {
            puffin::profile_scope!("dialog_manager_init_window");
            match DialogWindow::new(
                event_loop,
                pending.dialog_type,
                Some(parent_window),
                ui_config,
            ) {
                Ok(dialog) => {
                    tracing::info!("对话框窗口已创建: {:?}", pending.dialog_type);
                    self.initializing.push((dialog, pending));
                }
                Err(e) => {
                    tracing::error!("创建对话框窗口失败: {}", e);
                }
            }
        }

        // 推进当前正在初始化的对话框一个阶段。
        if let Some((dialog, pending)) = self.initializing.first_mut() {
            if dialog.gfx_ref().is_none() {
                // 阶段 2：创建 GFX。
                puffin::profile_scope!("dialog_manager_init_gfx");
                if let Err(e) = dialog.initialize_gfx() {
                    tracing::error!("初始化对话框 GFX 失败: {}", e);
                    self.initializing.remove(0);
                }
            } else {
                // 阶段 3：创建 UI 并显示窗口。
                puffin::profile_scope!("dialog_manager_init_ui");
                if let Err(e) = dialog.initialize_ui(
                    ui_config,
                    main_ui,
                    pending.pending_path.as_deref(),
                    pending.pending_size_mb.unwrap_or(0.0),
                    pending.pending_title.as_deref(),
                ) {
                    tracing::error!("初始化对话框 UI 失败: {}", e);
                    self.initializing.remove(0);
                } else {
                    let (dialog, _) = self.initializing.remove(0);
                    let window_id = dialog.window_id();
                    tracing::info!("对话框已就绪: {:?}", dialog.dialog_type);
                    self.dialogs.insert(window_id, dialog);
                }
            }
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

    /// 将主窗口的云存储 UI 状态广播到所有相关对话框
    ///
    /// 云存储唯一数据源是主窗口 Root（连接快照/目录列表/提醒由 runner 注入），
    /// 设置面板云管理页与云文件浏览器为独立 Root，需在此同步最新快照，
    /// 否则对话框内看不到已连接的设备。
    pub fn sync_cloud_to_dialogs(&mut self, main_ui: &lumino_ui::Host) {
        for dialog in self.dialogs.values_mut() {
            let relevant = matches!(
                dialog.dialog_type,
                DialogType::Settings
                    | DialogType::CloudConnect
                    | DialogType::CloudBrowser
                    | DialogType::CloudNotice
            );
            if relevant && let Some(ui) = dialog.ui_mut() {
                ui.sync_cloud_state_from(main_ui);
            }
        }
    }

    /// 将主题同步到所有已打开的对话框窗口
    ///
    /// 对话框主题在创建时从配置快照读取，运行中切换主题后若不广播，
    /// 对话框仍使用旧主题渲染（背景与文字可能与新主题不一致）。
    pub fn update_theme_all(&mut self, theme: String) {
        for dialog in self.dialogs.values_mut() {
            if let Some(ui) = dialog.ui_mut() {
                ui.update_theme(theme.clone());
            }
        }
    }

    /// 查找指定类型的第一个对话框窗口 ID
    ///
    /// 用于 Runner 在对话框 UI 就绪后注入数据（如 RecoverTrack 对话框的条目列表）。
    /// 返回 `None` 表示该类型对话框尚未就绪（仍在初始化或不存在）。
    pub fn first_dialog_id_of_type(&self, dialog_type: DialogType) -> Option<WindowId> {
        self.dialogs
            .values()
            .find(|d| d.dialog_type == dialog_type)
            .map(|d| d.window_id())
    }

    /// 转发视频导出进度到 VideoExport 对话框
    pub fn forward_video_export_progress(
        &mut self,
        message: String,
        progress: f64,
        total_frames: u64,
        render_fps: f64,
        elapsed_secs: f64,
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
                    elapsed_secs,
                );
                dialog.request_redraw();
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
                dialog.request_redraw();
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
                dialog.request_redraw();
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
                dialog.request_redraw();
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
