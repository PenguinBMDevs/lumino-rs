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
    /// 素材库"上传到云"（上传指定素材文件到目标目录）
    UploadMaterial,
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
                    "material_upload" => CloudIntent::UploadMaterial,
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
                // 云浏览状态唯一数据源为主窗口 Root：对话框的导航操作
                // （切设备/进目录/返回/刷新）经事件回传并更新主窗口，
                // 保证后续广播（snapshot 覆盖）值与对话框一致——
                // 否则保存模式切换文件夹后会被广播覆盖回根目录。
                {
                    let state = self.window_state.window.ui_mut().cloud_state_mut();
                    state.selected_id = Some(id.clone());
                    state.current_path = path.clone();
                    state.busy = true;
                    state.notice = None;
                    state.entries.clear();
                }
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
            cloud_event::Event::UploadMaterialRequest {
                id,
                dir_path,
                local_path,
                file_name,
            } => self.run_cloud_upload_material(id, dir_path, local_path, file_name),
            cloud_event::Event::NewFolderRequest { id, parent, name } => {
                self.run_cloud_new_folder(id, parent, name);
            }
            cloud_event::Event::RenameRequest { id, from, to } => {
                self.run_cloud_rename(id, from, to);
            }
            cloud_event::Event::DeleteRequest { id, path, is_dir } => {
                self.run_cloud_delete(id, path, is_dir);
            }
            cloud_event::Event::MoveRequest { id, from, to_dir } => {
                self.run_cloud_move(id, from, to_dir);
            }
            cloud_event::Event::CopyRequest {
                id,
                from,
                to_dir,
                is_cut,
            } => self.run_cloud_copy(id, from, to_dir, is_cut),
            cloud_event::Event::OpenConnectPanel => {
                self.window_state
                    .dialog_manager
                    .open_dialog(crate::runner::dialog_manager::DialogType::CloudConnect);
            }
            cloud_event::Event::OpenBrowserPanel { intent } => {
                let intent = match intent.as_str() {
                    "save" => CloudIntent::Save,
                    "material" => CloudIntent::Material,
                    "material_upload" => CloudIntent::UploadMaterial,
                    _ => CloudIntent::Import,
                };
                self.refresh_cloud_connections();
                self.open_cloud_browser(intent);
            }
            cloud_event::Event::ConnectExisting { id } => {
                self.run_cloud_connect_existing(id);
            }
            cloud_event::Event::DeleteConnection { id } => {
                let mgr = Arc::clone(&self.cloud);
                let id_for_ui = id.clone();
                std::thread::spawn(move || {
                    let mut mgr = lock_cloud(&mgr);
                    if let Err(e) = mgr.remove_connection(&id) {
                        tracing::warn!("删除云连接失败 {id}: {e}");
                    }
                });
                // 立即刷新 UI 快照
                self.window_state.window.ui_mut().cloud_state_mut().notice =
                    Some("连接已删除".to_string());
                self.refresh_cloud_connections();
                let _ = id_for_ui;
            }
            cloud_event::Event::DismissAlert => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(crate::runner::dialog_manager::DialogType::CloudNotice);
            }
            cloud_event::Event::AutoConnectFinished => {
                // 启动自动连接完成：静默刷新 UI 快照（不弹提醒，仅显示状态标志）
                self.refresh_cloud_connections();
            }

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
                id,
                remote_path,
                ok,
                error,
                local_path,
            } => self.apply_cloud_download_result(id, remote_path, ok, error, local_path),
            cloud_event::Event::SaveToCloudResult { ok, error } => {
                self.apply_cloud_save_result(ok, error);
            }
            cloud_event::Event::UploadMaterialResult { ok, error } => {
                self.apply_cloud_upload_result(ok, error);
            }
            cloud_event::Event::OperationResult { ok, error, kind } => {
                self.apply_cloud_operation_result(ok, error, kind);
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
            // 保存模式：文件菜单"保存到云"（工程）与素材库"上传到云"（素材文件）
            CloudIntent::Save | CloudIntent::UploadMaterial => (None, true),
            CloudIntent::Import => (None, false),
        };
        let auto_list_id = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            // 非素材上传入口：清除素材上传待办（防止残留导致"保存到云"
            // 误把上一次未完成的素材上传当成当前上传目标）
            if intent != CloudIntent::UploadMaterial {
                state.pending_upload = None;
            }
            state.filter = filter;
            state.save_mode = save_mode;
            state.notice = None;
            state.entries.clear();
            state.current_path = String::new();
            // 自动列出当前选中（在线）设备的根目录，避免打开后空目录
            state.selected_id.clone()
        };
        if let Some(id) = auto_list_id {
            event::emit(event::Event::cloud(cloud_event::Event::ListDirRequest {
                id,
                path: String::new(),
            }));
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
        let mgr = Arc::clone(&self.cloud);
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
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.connecting = false;
            if ok {
                state.connect_error = None;
                state.selected_id = Some(id);
                false
            } else {
                state.connect_error = Some(error.clone().unwrap_or_else(|| "未知错误".to_string()));
                true
            }
        };
        if failed {
            self.report_cloud_error("云存储连接失败", error.as_deref());
            return;
        }
        // 连接成功：清除历史提醒标志（状态实时更新）
        {
            let ui = self.window_state.window.ui_mut();
            ui.cloud_state_mut().alert_message = None;
            ui.settings_mut().cloud.alert = None;
        }
        self.refresh_cloud_connections();
        self.window_state
            .dialog_manager
            .mark_dialog_for_close(crate::runner::dialog_manager::DialogType::CloudConnect);
        let intent = self.cloud_intent.take().unwrap_or(CloudIntent::Import);
        self.open_cloud_browser(intent);
    }

    /// 断开连接（后台执行）
    fn run_cloud_disconnect(&mut self, id: String) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            lock_cloud(&mgr).disconnect(&id);
        });
        // 立即刷新 UI 快照（断开是本地操作，无需等线程）
        self.refresh_cloud_connections();
    }

    /// 后台连接已保存的指定连接，结果回传
    fn run_cloud_connect_existing(&mut self, id: String) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.connect(&id);
            event::emit(event::Event::cloud(cloud_event::Event::ConnectResult {
                id,
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            }));
        });
    }
}
