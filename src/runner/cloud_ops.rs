//! Runner 云存储事件处理 — 文件操作（列目录/下载/保存/新建文件夹）
//!
//! 所有耗时操作在后台线程执行（锁内调用 CloudManager），
//! 结果通过全局事件缓冲回传主线程，再注入 UI 状态。

use std::path::Path;

use lumino_ui::event::{self, cloud as cloud_event};
use lumino_ui::state::cloud_state::CloudEntryUi;

use crate::runner::RunnerInner;
use crate::storage;

use super::cloud::lock_cloud;

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

        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.download(&id, &remote_path, &local);
            let ok = result.is_ok();
            let error = result.err().map(|e| e.to_string());
            event::emit(event::Event::cloud(cloud_event::Event::DownloadResult {
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

    // ── 保存到云 ──

    /// 后台执行：导出当前工程归档到临时目录 → 上传到云目标目录
    pub(super) fn run_cloud_save(&mut self, id: String, dir_path: String) {
        // 构建工程（借 UI document，与导出工程一致）
        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            match data.document.as_ref() {
                Some(doc) => lumino_export::LuminoProject::from_midi_document(doc),
                None => {
                    self.apply_cloud_save_result(
                        false,
                        Some("当前没有可保存的工程内容".to_string()),
                    );
                    return;
                }
            }
        };
        let key_count = if self.window_state.storage.config.get().ui.enable_256key {
            256
        } else {
            128
        };
        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .and_then(|p| Path::new(p).file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".to_string());

        let tmp_dir = storage::config_dir().join("cloud_tmp");
        let local_path = tmp_dir.join(format!("{file_stem}.lmpj"));

        let mgr = self.cloud.clone();
        std::thread::spawn(move || {
            // 导出归档到临时文件
            if let Err(e) = lumino_export::save_project_to_folder_with_entry(
                &project,
                local_path.clone(),
                key_count,
            ) {
                event::emit(event::Event::cloud(cloud_event::Event::SaveToCloudResult {
                    ok: false,
                    error: Some(format!("导出工程失败：{e}")),
                }));
                return;
            }
            // 上传到云目标目录
            let remote = if dir_path.is_empty() {
                format!("{file_stem}.lmpj")
            } else {
                format!(
                    "{}/{}",
                    dir_path.trim_end_matches('/'),
                    local_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("{file_stem}.lmpj"))
                )
            };
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.upload(&id, &local_path, &remote);
            let _ = std::fs::remove_file(&local_path);
            event::emit(event::Event::cloud(cloud_event::Event::SaveToCloudResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            }));
        });
    }

    /// 注入保存结果
    pub(super) fn apply_cloud_save_result(&mut self, ok: bool, error: Option<String>) {
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                state.notice = Some("已保存到云存储".to_string());
                false
            } else {
                state.notice = Some(format!("保存失败：{}", error.clone().unwrap_or_default()));
                true
            }
        };
        if failed {
            self.notify_cloud_failure(format!("云存储连接异常（{}）", error.unwrap_or_default()));
        } else {
            self.sync_cloud_to_dialogs();
        }
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
            }));
        });
    }

    /// 注入通用操作结果：成功刷新列表，失败提示
    pub(super) fn apply_cloud_operation_result(&mut self, ok: bool, error: Option<String>) {
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                state.notice = Some("操作成功".to_string());
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

/// 系统下载目录（回退到配置目录 Downloads）
fn download_dir() -> std::path::PathBuf {
    directories::UserDirs::new()
        .and_then(|d| d.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| storage::config_dir().join("Downloads"))
}

/// 从远程路径提取文件名
fn remote_file_name(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => trimmed[idx + 1..].to_string(),
        None => trimmed.to_string(),
    }
}
