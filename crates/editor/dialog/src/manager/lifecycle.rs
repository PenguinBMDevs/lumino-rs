//! `DialogManager` 生命周期实现 — 创建、分帧初始化、关闭与每帧更新
//!
//! 自 `manager.rs` 拆分而来（零逻辑变更）。

use std::collections::{HashMap, HashSet};
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
            claimed_windows: HashSet::new(),
        }
    }

    /// 认领窗口归属（阶段 1 建窗成功后立即调用）
    ///
    /// REF-001 P0-1：认领后该窗口的所有 winit 事件都会被判定为「对话框窗口」，
    /// 不再落到主窗口分支（即使 UI 尚未就绪，`RunnerInner::handle_dialog_event`
    /// 也会安全地消费掉这些事件）。
    pub(crate) fn claim_window(&mut self, window_id: WindowId) {
        self.claimed_windows.insert(window_id);
    }

    /// 释放窗口归属（关闭对话框或初始化失败清理时调用）
    ///
    /// 与 [`Self::claim_window`] 成对出现：漏掉任何一条释放路径都会让
    /// `claimed_windows` 只增不减。
    pub(crate) fn release_window(&mut self, window_id: WindowId) {
        self.claimed_windows.remove(&window_id);
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
                    // REF-001 P0-1：建窗成功即认领窗口归属——此后（含阶段 2/3
                    // 的整个初始化窗口期）该窗口的事件都不会再落到主窗口分支。
                    let window_id = dialog.window_id();
                    self.claim_window(window_id);
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
            let window_id = dialog.window_id();
            if dialog.gfx_ref().is_none() {
                // 阶段 2：创建 GFX。
                puffin::profile_scope!("dialog_manager_init_gfx");
                if let Err(e) = dialog.initialize_gfx() {
                    tracing::error!("初始化对话框 GFX 失败: {}", e);
                    self.initializing.remove(0);
                    // 窗口随 DialogWindow 一起销毁，必须释放归属：
                    // claimed_windows 只增不减会泄漏，且窗口 id 不会复用。
                    self.release_window(window_id);
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
                    self.release_window(window_id);
                } else {
                    let (dialog, _) = self.initializing.remove(0);
                    tracing::info!("对话框已就绪: {:?}", dialog.dialog_type);
                    // 归属保留：窗口从 initializing 迁到 dialogs，仍属对话框窗口。
                    self.dialogs.insert(window_id, dialog);
                }
            }
        }
    }
}

impl DialogManager {
    /// 关闭对话框
    pub fn close_dialog(&mut self, window_id: WindowId) {
        // 无论该窗口处于哪个阶段都要释放归属：只关 dialogs 里的窗口会让
        // claimed_windows 残留（事件仍被判为对话框窗口，且集合无界增长）。
        self.release_window(window_id);
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

            // REF-001：入册后的首帧完成「测量 → resize → 居中 → 显示」，只做一次。
            // 放在这里而不是 `initialize_ui` 末尾，有两个硬理由：
            // 1. 同帧稍早的 `try_fill_recover_track_entries` 已完成内容注入
            //    （`src/runner/lifecycle/handler.rs`），此时测量才看得到完整内容；
            // 2. 窗口已入册 ⇒ resize 回程的 `Resized` 会正确路由回本窗口。
            dialog.apply_measured_size();

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
