//! `DialogManager` 生命周期实现 — 创建、分帧初始化、关闭与每帧更新
//!
//! 自 `manager.rs` 拆分而来（零逻辑变更）。

use std::collections::HashMap;
use std::sync::Arc;

use lumino_core::BrushConfig;
use lumino_core::storage::config::UiConfig;
use lumino_ui::state::root_state::DialogType;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use crate::window::DialogWindow;

use super::{DialogManager, PendingDialog};

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
            pending_brush_config: None,
        });
    }

    /// 请求打开工程设置对话框（带自定义标题）
    pub fn open_project_settings(&mut self, title: String) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::ProjectSettings,
            pending_path: None,
            pending_size_mb: None,
            pending_title: Some(title),
            pending_brush_config: None,
        });
    }

    /// 请求打开加载确认对话框
    pub fn open_load_confirm(&mut self, path: String, size_mb: f64) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::LoadConfirm,
            pending_path: Some(path),
            pending_size_mb: Some(size_mb),
            pending_title: None,
            pending_brush_config: None,
        });
    }

    /// 请求打开画刷「绘制行为」对话框（携带当前画刷配置）
    pub fn open_brush_settings(&mut self, config: BrushConfig) {
        self.pending_dialogs.push(PendingDialog {
            dialog_type: DialogType::BrushSettings,
            pending_path: None,
            pending_size_mb: None,
            pending_title: None,
            pending_brush_config: Some(config),
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
            pending_brush_config: None,
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
                    pending.pending_brush_config.as_ref(),
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
}

impl DialogManager {
    /// 关闭对话框
    pub fn close_dialog(&mut self, window_id: WindowId) {
        if self.dialogs.remove(&window_id).is_some() {
            tracing::info!("对话框已关闭: {:?}", window_id);
        }
    }
}

impl DialogManager {
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
