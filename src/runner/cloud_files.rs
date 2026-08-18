//! Runner 云存储文件操作 — 重命名/删除/移动/复制（后台线程执行）
//!
//! 从 `cloud_ops` 拆分而来（文件长度红线 <400 行）：
//! - 后台执行 CloudManager 的文件操作
//! - 复制 = 下载到本地临时 + 上传（不依赖服务器端 COPY 支持）
//! - 结果统一经 `OperationResult`（带 kind）回传注入

use std::sync::Arc;

use lumino_ui::event::{self, cloud as cloud_event};

use crate::runner::RunnerInner;
use crate::storage;

use super::cloud::lock_cloud;

impl RunnerInner {
    /// 后台重命名（同目录），结果回传
    pub(super) fn run_cloud_rename(&mut self, id: String, from: String, to: String) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.rename(&id, &from, &to);
            event::emit(event::Event::cloud(cloud_event::Event::OperationResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                kind: "rename".to_string(),
            }));
        });
    }

    /// 后台删除文件/目录，结果回传
    pub(super) fn run_cloud_delete(&mut self, id: String, path: String, is_dir: bool) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.delete(&id, &path, is_dir);
            event::emit(event::Event::cloud(cloud_event::Event::OperationResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                kind: "delete".to_string(),
            }));
        });
    }

    /// 后台移动（剪切，云内部），结果回传
    pub(super) fn run_cloud_move(&mut self, id: String, from: String, to_dir: String) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.move_file(&id, &from, &to_dir);
            event::emit(event::Event::cloud(cloud_event::Event::OperationResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                kind: "move".to_string(),
            }));
        });
    }

    /// 后台复制/剪切：剪切 = 云内部移动；复制 = 下载到临时 + 上传（跨协议通用）
    pub(super) fn run_cloud_copy(
        &mut self,
        id: String,
        from: String,
        to_dir: String,
        is_cut: bool,
    ) {
        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = if is_cut {
                mgr.move_file(&id, &from, &to_dir)
            } else {
                copy_via_tmp(&mut mgr, &id, &from, &to_dir)
            };
            event::emit(event::Event::cloud(cloud_event::Event::OperationResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                kind: if is_cut {
                    "paste_cut".to_string()
                } else {
                    "paste_copy".to_string()
                },
            }));
        });
    }

    /// 注入通用操作结果：成功刷新列表，失败提示
    pub(super) fn apply_cloud_operation_result(
        &mut self,
        ok: bool,
        error: Option<String>,
        kind: String,
    ) {
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                state.notice = Some("操作成功".to_string());
                // 剪切粘贴成功：源已移动，清空剪贴板（复制粘贴保留，可重复粘贴）
                if kind == "paste_cut" {
                    state.clipboard = None;
                }
                false
            } else {
                state.notice = Some(format!("操作失败：{}", error.clone().unwrap_or_default()));
                true
            }
        };
        if failed {
            self.notify_cloud_failure(format!("云存储连接异常（{}）", error.unwrap_or_default()));
        } else {
            // 刷新当前目录列表
            let id = self
                .window_state
                .window
                .ui()
                .cloud_state()
                .selected_id
                .clone();
            let path = self
                .window_state
                .window
                .ui()
                .cloud_state()
                .current_path
                .clone();
            if let Some(id) = id {
                self.run_cloud_list(id, path);
            }
            self.sync_cloud_to_dialogs();
        }
    }
}

/// 复制 = 下载到本地临时文件 + 上传到目标目录（不依赖服务器端 COPY 支持）
fn copy_via_tmp(
    mgr: &mut lumino_cloud::CloudManager,
    id: &str,
    from: &str,
    to_dir: &str,
) -> lumino_cloud::Result<()> {
    let name = remote_file_name(from);
    let tmp_dir = storage::config_dir().join("cloud_tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(lumino_cloud::CloudError::Io)?;
    let tmp_path = tmp_dir.join(format!(
        "copy_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let result = (|| {
        mgr.download(id, from, &tmp_path)?;
        let remote = if to_dir.is_empty() || to_dir == "/" {
            name
        } else {
            format!("{}/{}", to_dir.trim_end_matches('/'), name)
        };
        mgr.upload(id, &tmp_path, &remote)
    })();

    let _ = std::fs::remove_file(&tmp_path);
    result
}

/// 系统下载目录（回退到配置目录 Downloads）
pub(super) fn download_dir() -> std::path::PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| storage::config_dir().join("Downloads"))
}

/// 从远程路径提取文件名
pub(super) fn remote_file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}
