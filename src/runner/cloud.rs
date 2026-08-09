//! Runner 云存储事件处理 — 入口分流与连接管理
//!
//! 职责：
//! - 持有 `CloudManager`（后台线程锁内执行耗时操作，避免阻塞事件循环）
//! - 云入口意图管理（从云导入/保存到云/素材导入）
//! - 连接/断开与连接快照注入
//! - 文件操作（列目录/下载/保存/新建文件夹）见 `cloud_ops` 模块

use std::sync::{Arc, Mutex};

use lumino_cloud::{CloudConnection, CloudManager, CloudProtocol};
use lumino_ui::event::{self, cloud as cloud_event};
use lumino_ui::state::cloud_state::CloudConnInfo;

use crate::runner::RunnerInner;

/// 云入口意图
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudIntent {
    /// 文件菜单"从云导入"（全类型）
    Import,
    /// 文件菜单"保存到云"（工程归档上传）
    Save,
    /// 素材库"从云导入"（仅 .lmmaterial）
    Material,
}

/// 锁并获取 CloudManager 可变引用（恢复 poisoned 锁）
pub(super) fn lock_cloud(
    mgr: &Arc<Mutex<CloudManager>>,
) -> std::sync::MutexGuard<'_, CloudManager> {
    mgr.lock().unwrap_or_else(|e| e.into_inner())
}

impl RunnerInner {
    // ── 事件分发 ──

    /// 处理云存储事件
    pub(super) fn handle_cloud_event(&mut self, ev: cloud_event::Event) {
        match ev {
            cloud_event::Event::OpenCloudPanel { intent } => {
                let intent = match intent.as_str() {
                    "save" => CloudIntent::Save,
                    "material" => CloudIntent::Material,
                    _ => CloudIntent::Import,
                };
                self.ensure_cloud_ready(intent);
            }
            cloud_event::Event::ConnectRequest {
                name,
                protocol,
                address,
                port,
                username,
                password,
            } => self.run_cloud_connect(name, protocol, address, port, username, password),
            cloud_event::Event::DisconnectRequest(id) => self.run_cloud_disconnect(id),
            cloud_event::Event::ListDirRequest { id, path } => {
                self.run_cloud_list(id, path);
            }
            cloud_event::Event::DownloadRequest {
                id,
                remote_path,
                target,
            } => self.run_cloud_download(id, remote_path, target),
            cloud_event::Event::SaveToCloudRequest { id, dir_path } => {
                self.run_cloud_save(id, dir_path);
            }
            cloud_event::Event::NewFolderRequest { id, parent, name } => {
                self.run_cloud_new_folder(id, parent, name);
            }
            cloud_event::Event::RenameRequest { .. } => {
                // 云管理面板操作（Phase 4 支持）
            }
            cloud_event::Event::DeleteRequest { .. } => {}
            cloud_event::Event::MoveRequest { .. } => {}

            // ── 结果回传（后台线程 → 本函数 → UI 注入） ──
            cloud_event::Event::ConnectResult { id, ok, error } => {
                self.apply_cloud_connect_result(id, ok, error);
            }
            cloud_event::Event::ListDirResult {
                id: _,
                path: _,
                entries,
                error,
            } => self.apply_cloud_list_result(entries, error),
            cloud_event::Event::DownloadResult {
                remote_path: _,
                ok,
                error,
                local_path,
            } => self.apply_cloud_download_result(ok, error, local_path),
            cloud_event::Event::SaveToCloudResult { ok, error } => {
                self.apply_cloud_save_result(ok, error);
            }
            cloud_event::Event::OperationResult { ok, error } => {
                self.apply_cloud_operation_result(ok, error);
            }
        }
    }

    // ── 入口分流 ──

    /// 云入口统一分流：无在线连接 → 打开连接面板；已连接 → 打开文件浏览面板
    pub(super) fn ensure_cloud_ready(&mut self, intent: CloudIntent) {
        self.cloud_intent = Some(intent);
        let has_online = lock_cloud(&self.cloud).online_ids().is_empty();
        if !has_online {
            self.refresh_cloud_connections();
            self.open_cloud_browser(intent);
        } else {
            self.window_state
                .dialog_manager
                .open_dialog(crate::runner::dialog_manager::DialogType::CloudConnect);
        }
    }

    /// 按意图打开云文件浏览面板
    fn open_cloud_browser(&mut self, intent: CloudIntent) {
        let (filter, save_mode) = match intent {
            CloudIntent::Material => (Some("lmmaterial".to_string()), false),
            CloudIntent::Save => (None, true),
            CloudIntent::Import => (None, false),
        };
        {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.filter = filter;
            state.save_mode = save_mode;
            state.notice = None;
            state.entries.clear();
            state.current_path = String::new();
        }
        self.window_state
            .dialog_manager
            .open_dialog(crate::runner::dialog_manager::DialogType::CloudBrowser);
    }

    // ── 连接 ──

    /// 后台线程执行连接（保存配置 + 连接），结果回传
    fn run_cloud_connect(
        &mut self,
        name: String,
        protocol: String,
        address: String,
        port: u16,
        username: String,
        password: String,
    ) {
        let mgr = self.cloud.clone();
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let protocol = match protocol.as_str() {
                "sftp" => CloudProtocol::Sftp,
                "webdav" => CloudProtocol::Webdav,
                _ => CloudProtocol::Ftp,
            };
            let conn = CloudConnection::new(
                name,
                protocol,
                address,
                (port != 0).then_some(port),
                username,
                lumino_cloud::crypto::encrypt(&password).unwrap_or_else(|e| e.to_string()),
                String::new(),
            );
            let conn_id = conn.id.clone();
            let _ = mgr.upsert_connection(conn);
            let result = mgr.connect(&conn_id);
            event::emit(event::Event::cloud(cloud_event::Event::ConnectResult {
                id: conn_id,
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            }));
        });
    }

    /// 注入连接结果：成功 → 关闭面板并打开浏览面板；失败 → 面板内显示原因
    fn apply_cloud_connect_result(&mut self, id: String, ok: bool, error: Option<String>) {
        {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.connecting = false;
            if ok {
                state.connect_error = None;
                state.selected_id = Some(id);
            } else {
                state.connect_error = Some(error.unwrap_or_else(|| "未知错误".to_string()));
            }
        }
        if ok {
            self.refresh_cloud_connections();
            self.window_state
                .dialog_manager
                .mark_dialog_for_close(crate::runner::dialog_manager::DialogType::CloudConnect);
            let intent = self.cloud_intent.take().unwrap_or(CloudIntent::Import);
            self.open_cloud_browser(intent);
        }
    }

    /// 断开连接（后台执行）
    fn run_cloud_disconnect(&mut self, id: String) {
        let mgr = self.cloud.clone();
        std::thread::spawn(move || {
            lock_cloud(&mgr).disconnect(&id);
        });
        // 立即刷新 UI 快照（断开是本地操作，无需等线程）
        self.refresh_cloud_connections();
    }

    // ── 连接快照 ──

    /// 将 CloudManager 的连接快照注入 UI（设备下拉 + 在线状态）
    pub(super) fn refresh_cloud_connections(&mut self) {
        let snapshot = {
            let mgr = lock_cloud(&self.cloud);
            mgr.connections()
                .iter()
                .map(|c| {
                    (
                        c.id.clone(),
                        c.name.clone(),
                        c.protocol.display_name().to_string(),
                        mgr.status(&c.id).is_online(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let ui = self.window_state.window.ui_mut();
        let state = ui.cloud_state_mut();
        state.connections = snapshot
            .into_iter()
            .map(|(id, name, protocol, online)| CloudConnInfo {
                id,
                name,
                protocol,
                online,
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
}
