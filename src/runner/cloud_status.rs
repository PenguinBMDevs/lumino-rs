//! Runner 云存储状态管理 — 启动自动连接 / 断连提醒 / 连接快照注入
//!
//! 从 `cloud` 拆分而来（文件长度红线 <400 行）：
//! - 启动自动连接（后台静默，失败仅日志 + 离线标志）
//! - 断连提醒统一入口（会话只弹一次独立面板）
//! - 连接快照注入主窗口 + 广播到已打开的设置/云对话框

use std::sync::Arc;

use lumino_ui::event::{self, cloud as cloud_event};
use lumino_ui::state::cloud_state::CloudConnInfo;

use crate::runner::RunnerInner;

use super::cloud::lock_cloud;

impl RunnerInner {
    // ── 启动自动连接 ──

    /// 应用启动后自动连接用户配置的云存储（需求 4 + Q8）
    ///
    /// 后台线程执行，逐个尝试 auto_connect 标记的连接；
    /// 失败仅记录日志并显示离线标志，**不弹提醒面板**（用户主动操作才提醒）。
    pub(super) fn startup_auto_connect(&self) {
        let mgr = Arc::clone(&self.cloud);
        let status = Arc::clone(&self.cloud_status);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let results = mgr.connect_all_auto();
            for (id, result) in &results {
                match result {
                    Ok(()) => tracing::info!("启动自动连接成功: {id}"),
                    Err(e) => tracing::warn!("启动自动连接失败 {id}: {e}"),
                }
            }
            // DEBT-06 #123：自动连接改变在线状态 → 先发布快照再通知 UI
            super::cloud::publish_cloud_snapshot(&status, &mgr);
            // 全部尝试完成后通知主线程刷新 UI 快照（静默，不弹提醒）
            event::emit(event::Event::cloud(cloud_event::Event::AutoConnectFinished));
        });
    }

    // ── 断连提醒 ──

    /// 云操作失败统一提醒入口（需求 6 + Q5）：
    /// 每次会话**只弹一次**独立提醒面板，之后仅在设置面板云管理页显示状态标志。
    pub(super) fn notify_cloud_failure(&mut self, reason: String) {
        // 更新设置面板标志（始终显示，实时更新）
        {
            let ui = self.window_state.window.ui_mut();
            ui.cloud_state_mut().alert_message = Some(reason.clone());
            ui.settings_mut().cloud.alert = Some(reason);
        }
        // 广播到已打开的设置/云对话框
        self.sync_cloud_to_dialogs();
        // 只弹一次独立提醒面板
        if !self.cloud_alert_shown {
            self.cloud_alert_shown = true;
            self.window_state
                .dialog_manager
                .open_dialog(crate::runner::dialog_manager::DialogType::CloudNotice);
        }
    }

    /// 格式化云操作错误原因：保留根因；错误为 `None` 时回退"未知错误"。
    ///
    /// 用于替代各 `apply_*` 中 `error.unwrap_or_default()` 的静默吞错写法，
    /// 杜绝线上排障时根因丢失。
    pub(super) fn cloud_error_text(context: &str, error: Option<&str>) -> String {
        match error {
            Some(e) => format!("{context}（{e}）"),
            None => format!("{context}（未知错误）"),
        }
    }

    /// 云操作失败统一提醒（保留根因，避免静默吞错）。
    ///
    /// 等价于 `self.notify_cloud_failure(Self::cloud_error_text(context, error))`。
    pub(super) fn report_cloud_error(&mut self, context: &str, error: Option<&str>) {
        self.notify_cloud_failure(Self::cloud_error_text(context, error));
    }

    // ── 连接快照 ──

    /// 将主窗口云存储快照广播到已打开的设置/云对话框（独立 Root 需同步）
    pub(super) fn sync_cloud_to_dialogs(&mut self) {
        let main_ui = self.window_state.window.ui();
        self.window_state
            .dialog_manager
            .sync_cloud_to_dialogs(main_ui);
    }

    /// 将云连接快照注入 UI（设备下拉 + 在线状态 + 设置面板云管理页）
    ///
    /// DEBT-06 #123：只读**无锁快照**，不再抢 `cloud` 业务锁——旧实现在大文件
    /// 传输期间被后台线程持锁，UI 线程打开设置页/云面板会冻结。
    pub(super) fn refresh_cloud_connections(&mut self) {
        let snapshot = super::cloud::read_cloud_snapshot(&self.cloud_status);
        let snapshot: Vec<super::cloud::CloudConnSnapshot> = snapshot.connections;
        let ui = self.window_state.window.ui_mut();
        // 文件浏览面板的设备下拉
        {
            let state = ui.cloud_state_mut();
            state.connections = snapshot
                .iter()
                .map(|c| CloudConnInfo {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    protocol: c.protocol.clone(),
                    online: c.online,
                })
                .collect();
            // 当前选中不在线 → 自动切换到第一个在线连接
            let selected_online = state.selected().map(|c| c.online).unwrap_or(false);
            if !selected_online {
                state.selected_id = state
                    .connections
                    .iter()
                    .find(|c| c.online)
                    .map(|c| c.id.clone());
            }
        }
        // 设置面板云管理页
        {
            let settings = ui.settings_mut();
            settings.cloud.connections = snapshot
                .into_iter()
                .map(|c| lumino_ui::settings::CloudConnItem {
                    id: c.id,
                    name: c.name,
                    protocol: c.protocol,
                    address: c.address,
                    online: c.online,
                })
                .collect();
        }
        // 广播到已打开的设置/云对话框（独立 Root 同步快照）
        self.sync_cloud_to_dialogs();
    }
}
