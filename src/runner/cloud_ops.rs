//! Runner 云存储事件处理 — 文件操作（列目录/下载/新建文件夹）
//!
//! 所有耗时操作在后台线程执行（锁内调用 CloudManager），
//! 结果通过全局事件缓冲回传主线程，再注入 UI 状态。
//! 保存到云 / 自动回传见 `cloud_save` 模块（长度红线拆分）。

use std::path::Path;

use lumino_ui::event::{self, cloud as cloud_event};
use lumino_ui::state::cloud_state::CloudEntryUi;

use crate::runner::RunnerInner;
use crate::storage;

use super::cloud::lock_cloud;
use super::cloud_files::{download_dir, remote_file_name};

impl RunnerInner {
    // ── 目录列表 ──

    /// 后台列出目录，结果回传
    pub(super) fn run_cloud_list(&mut self, id: String, path: String) {
        let mgr = self.cloud.clone();
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.list_dir(&id, &path);
            let (entries, error) = match result {
                Ok(entries) => (
                    entries
                        .into_iter()
                        .map(|e| cloud_event::RemoteEntry {
                            name: e.name,
                            path: e.path,
                            is_dir: e.is_dir,
                            size: e.size,
                        })
                        .collect(),
                    None,
                ),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            event::emit(event::Event::cloud(cloud_event::Event::ListDirResult {
                id,
                path,
                entries,
                error,
            }));
        });
    }

    /// 注入目录列表结果
    pub(super) fn apply_cloud_list_result(
        &mut self,
        entries: Vec<cloud_event::RemoteEntry>,
        error: Option<String>,
    ) {
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if let Some(err) = error {
                state.notice = Some(format!("加载失败：{err}"));
                true
            } else {
                state.entries = entries
                    .into_iter()
                    .map(|e| CloudEntryUi {
                        name: e.name,
                        path: e.path,
                        is_dir: e.is_dir,
                        size: e.size,
                        modified: None,
                    })
                    .collect();
                false
            }
        };
        if failed {
            self.notify_cloud_failure("云存储连接异常".to_string());
        } else {
            // 目录列表已更新：广播到已打开的云文件浏览器对话框
            self.sync_cloud_to_dialogs();
        }
    }

    // ── 下载 ──

    /// 后台下载：按入口类型决定落点（素材目录 / 下载目录）
    pub(super) fn run_cloud_download(
        &mut self,
        id: String,
        remote_path: String,
        target: cloud_event::DownloadTarget,
    ) {
        let mgr = self.cloud.clone();
        let local = match target {
            cloud_event::DownloadTarget::Material => storage::config_dir().join("Materials"),
            cloud_event::DownloadTarget::Import => download_dir(),
        }
        .join(remote_file_name(&remote_path));

        // 云进度悬浮窗：开始下载
        let progress_tx = self.window_state.cloud_progress_tx.clone();
        let file_name = remote_file_name(&remote_path);
        let _ = progress_tx.send((format!("正在下载 {file_name}"), 0.1));

        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.download(&id, &remote_path, &local);
            let ok = result.is_ok();
            let error = result.err().map(|e| e.to_string());
            // 云进度悬浮窗：下载结束（完成/失败均关闭）
            let done_msg = if ok {
                format!("下载完成：{file_name}")
            } else {
                format!("下载失败：{file_name}")
            };
            let _ = progress_tx.send((done_msg, 1.0));
            event::emit(event::Event::cloud(cloud_event::Event::DownloadResult {
                id,
                remote_path,
                ok,
                error,
                local_path: ok.then(|| local.to_string_lossy().into_owned()),
            }));
        });
    }

    /// 注入下载结果：素材 → 重新扫描素材库；MIDI → 导入工程；其他 → 提示
    pub(super) fn apply_cloud_download_result(
        &mut self,
        id: String,
        remote_path: String,
        ok: bool,
        error: Option<String>,
        local_path: Option<String>,
    ) {
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                false
            } else {
                state.notice = Some(format!("下载失败：{}", error.clone().unwrap_or_default()));
                true
            }
        };
        if failed {
            self.notify_cloud_failure(format!("云存储连接异常（{}）", error.unwrap_or_default()));
            return;
        }
        let Some(local) = local_path else { return };
        let path = Path::new(&local);
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "lmmaterial" => {
                // 已落入用户素材目录，触发重新扫描
                self.window_state.window.ui_mut().request_material_scan();
                self.window_state.window.ui_mut().cloud_state_mut().notice =
                    Some("素材已导入素材库".to_string());
            }
            "lmpj" => {
                // 工程归档（单文件 LMPJ）→ 自动加载到编辑器，并记录云端来源：
                // 保存后自动上传回原远程路径
                self.midi_state.cloud_source = Some(super::inner::CloudSource {
                    conn_id: id,
                    remote_path,
                });
                self.load_midi_file(path.to_path_buf());
                self.window_state.window.ui_mut().cloud_state_mut().notice =
                    Some("已下载并导入工程".to_string());
            }
            "mid" | "midi" => {
                self.load_midi_file(path.to_path_buf());
            }
            _ => {
                self.window_state.window.ui_mut().cloud_state_mut().notice =
                    Some(format!("已下载到 {}", path.display()));
            }
        }
        // 广播到已打开的设置/云对话框
        self.sync_cloud_to_dialogs();
    }

    // ── 新建文件夹 ──

    /// 后台新建文件夹，结果回传
    pub(super) fn run_cloud_new_folder(&mut self, id: String, parent: String, name: String) {
        let mgr = self.cloud.clone();
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let path = if parent.is_empty() || parent == "/" {
                format!("/{name}")
            } else {
                format!("{}/{name}", parent.trim_end_matches('/'))
            };
            let result = mgr.create_dir(&id, &path);
            event::emit(event::Event::cloud(cloud_event::Event::OperationResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
                kind: "new_folder".to_string(),
            }));
        });
    }
}
