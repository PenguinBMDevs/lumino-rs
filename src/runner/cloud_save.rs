//! Runner 云存储保存操作 — 保存到云 / Ctrl+S 自动回传
//!
//! 从 `cloud_ops` 拆分而来（文件长度红线 <400 行）：
//! - `run_cloud_save`：手动"保存到云"（导出归档 → 上传到目标目录）
//! - `run_cloud_upload_overwrite`：云端工程 Ctrl+S 后的自动回传
//!   （三步走防冲突：临时名上传 → 删原文件 → 重命名）
//! - 云上传严格串行：`cloud_saving` 置位期间新上传请求直接拒绝
//! - 进度经 `cloud_progress_tx` 推送到覆盖型悬浮窗

use std::path::Path;
use std::sync::Arc;

use lumino_ui::event::{self, cloud as cloud_event};

use crate::runner::RunnerInner;
use crate::storage;

use super::cloud::lock_cloud;

impl RunnerInner {
    // ── 保存到云（手动） ──

    /// 后台执行：导出当前工程归档到临时目录 → 上传到云目标目录
    pub(super) fn run_cloud_save(&mut self, id: String, dir_path: String) {
        // 云上传串行限制：已有上传进行中则忽略（避免并发写云导致文件混乱）
        if self.cloud_saving.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::warn!("云上传进行中，忽略保存到云请求");
            self.window_state.window.ui_mut().cloud_state_mut().notice =
                Some("正在上传中，请稍候再试".to_string());
            return;
        }
        // 上传进行中：禁止关闭软件（完成后在 apply_cloud_save_result 清除）
        self.cloud_saving
            .store(true, std::sync::atomic::Ordering::SeqCst);
        // 构建工程（借 UI document，与导出工程一致）
        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            // 抓取工程设置对话框中的作者/版权（关闭工程后由 Runner 从已加载
            // .lmpj 回填），随云保存一并持久化，避免重新下载后丢失
            let author = ui.get_project_author();
            let copyright = ui.get_project_copyright();
            match data.document.as_ref() {
                Some(doc) => {
                    let mut project = lumino_export::LuminoProject::from_midi_document(doc);
                    // 用编辑器 tempo_points 覆盖 doc 的加载时原始 tempo，
                    // 保证用户修改的 BPM（工程设置/速度面板）随云保存持久化
                    project
                        .apply_tempo_points(data.tempo_points.iter().map(|tp| (tp.tick, tp.bpm)));
                    // 累计创作时间随云保存持久化（与本地保存一致）
                    project.set_working_time_seconds(self.session_tracker.current_editing_secs());
                    // 作者/版权随云保存持久化（与本地保存一致）
                    project.metadata.project.author = author;
                    project.metadata.project.copyright = copyright;
                    project
                }
                None => {
                    self.apply_cloud_save_result(
                        false,
                        Some("当前没有可保存的工程内容".to_string()),
                    );
                    return;
                }
            }
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

        // 云进度悬浮窗：开始保存到云（导出阶段）
        let progress_tx = self.window_state.cloud_progress_tx.clone();
        let _ = progress_tx.send((format!("正在导出工程 {file_stem}.lmpj"), 0.1));

        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            // 导出工程为**单文件归档**（LMPJ 魔数开头，完整包含全部音轨数据）。
            // 注意：不得使用 save_project_to_folder_with_entry——它生成 67 字节
            // 入口文件 + 同目录数据文件夹，云上传只有单文件通道，
            // 数据文件夹不会上传，下载后工程无法加载（曾引发 67 字节 bug）。
            if let Err(e) = lumino_export::save_to_archive(&project, local_path.clone()) {
                let _ = progress_tx.send((format!("导出工程失败：{e}"), 1.0));
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
            let _ = progress_tx.send((format!("正在上传 {file_stem}.lmpj 到云存储"), 0.5));
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.upload(&id, &local_path, &remote);
            let _ = std::fs::remove_file(&local_path);
            let done_msg = if result.is_ok() {
                "已保存到云存储".to_string()
            } else {
                format!(
                    "保存到云失败：{}",
                    result
                        .as_ref()
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "未知错误".to_string())
                )
            };
            let _ = progress_tx.send((done_msg, 1.0));
            event::emit(event::Event::cloud(cloud_event::Event::SaveToCloudResult {
                ok: result.is_ok(),
                error: result.err().map(|e| e.to_string()),
            }));
        });
    }

    // ── 自动回传（Ctrl+S 后云端工程覆盖上传） ──

    /// 自动回传：将已保存的本地工程文件安全上传回云端原路径
    ///
    /// **三步走防冲突**（避免直接覆盖时传输中断损坏云端文件）：
    /// 1. 上传到临时名（原路径 + `.saving` 后缀，如 `project.lmpj.saving`）
    /// 2. 上传成功后删除云端原文件（旧版本）
    /// 3. 将临时文件重命名为正确的原路径
    ///
    /// **失败分级处理**：
    /// - 上传/删除阶段失败：清理临时文件，云端原文件完好保留
    /// - 重命名阶段失败：**保留临时文件**（此时原文件已删除，临时文件是云端唯一副本，避免数据丢失）
    ///
    /// **串行限制**：上传期间 `cloud_saving` 置位，其他保存/上传请求被直接拒绝；
    /// 结果经 `SaveToCloudResult` 回传，由 `apply_cloud_save_result` 清除标志。
    pub(super) fn run_cloud_upload_overwrite(
        &mut self,
        id: String,
        remote_path: String,
        local_path: std::path::PathBuf,
    ) {
        // 云上传串行限制：已有上传进行中则忽略本次回传
        if self.cloud_saving.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::warn!("云上传进行中，忽略本次自动回传");
            return;
        }
        self.cloud_saving
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // 临时名：原路径追加 `.saving`（同目录内，重命名无需跨目录）
        let tmp_remote = overwrite_tmp_path(&remote_path);

        // 云进度悬浮窗：开始自动回传
        let progress_tx = self.window_state.cloud_progress_tx.clone();
        let _ = progress_tx.send((format!("正在上传 {tmp_remote}"), 0.2));

        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            let mut mgr = lock_cloud(&mgr);
            // 失败阶段标记：0=上传失败 / 1=删除失败 / 2=重命名失败
            let mut stage: u8 = 0;
            let result = (|| {
                // 1. 上传到临时名
                mgr.upload(&id, &local_path, &tmp_remote)?;
                let _ = progress_tx.send(("上传完成，正在更新云端文件".to_string(), 0.7));
                // 2. 删除云端原文件（旧版本）
                if let Err(e) = mgr.delete(&id, &remote_path, false) {
                    tracing::warn!("删除云端旧文件失败（保留原文件）: {e}");
                    stage = 1;
                    return Err(e);
                }
                let _ = progress_tx.send(("正在完成云端重命名".to_string(), 0.9));
                // 3. 重命名为正确的文件名
                match mgr.rename(&id, &tmp_remote, &remote_path) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        stage = 2;
                        Err(e)
                    }
                }
            })();

            // 失败清理：仅在上传/删除阶段失败时删除临时文件；
            // 重命名失败时保留临时文件（云端唯一副本，防数据丢失）
            if result.is_err() && stage != 2 {
                let _ = mgr.delete(&id, &tmp_remote, false);
            }

            let error = match (&result, stage) {
                (Err(e), 2) => Some(format!("上传后重命名失败，文件已保留为 {tmp_remote}：{e}")),
                (Err(e), 1) => Some(format!("删除云端旧文件失败：{e}")),
                (Err(e), _) => Some(format!("上传到云端失败：{e}")),
                (Ok(()), _) => None,
            };
            // 云进度悬浮窗：结束（完成/失败均关闭）
            let done_msg = if result.is_ok() {
                "已保存到云存储".to_string()
            } else {
                Self::cloud_error_text("保存到云失败", error.as_deref())
            };
            let _ = progress_tx.send((done_msg, 1.0));
            event::emit(event::Event::cloud(cloud_event::Event::SaveToCloudResult {
                ok: result.is_ok(),
                error,
            }));
        });
    }

    // ── 上传素材到云（素材库右键"上传到云"） ──

    /// 后台上传素材文件到云目标目录
    ///
    /// 与 `run_cloud_save`（导出工程归档）区分：本方法直接上传指定本地
    /// 素材文件（.lmmaterial）。仅用户素材可上传（内置素材不支持）。
    pub(super) fn run_cloud_upload_material(
        &mut self,
        id: String,
        dir_path: String,
        local_path: String,
        file_name: String,
    ) {
        // 云上传串行限制：已有上传进行中则忽略（避免并发写云导致文件混乱）
        if self.cloud_saving.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::warn!("云上传进行中，忽略素材上传请求");
            self.window_state.window.ui_mut().cloud_state_mut().notice =
                Some("正在上传中，请稍候再试".to_string());
            return;
        }
        // 上传进行中：禁止关闭软件（完成后在 apply_cloud_upload_result 清除）
        self.cloud_saving
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // 云进度悬浮窗：开始上传
        let progress_tx = self.window_state.cloud_progress_tx.clone();
        let _ = progress_tx.send((format!("正在上传素材 {file_name}"), 0.2));

        let mgr = Arc::clone(&self.cloud);
        std::thread::spawn(move || {
            // 远程路径 = 目标目录 / 文件名
            let remote = if dir_path.is_empty() {
                file_name.clone()
            } else {
                format!("{}/{}", dir_path.trim_end_matches('/'), file_name)
            };
            let mut mgr = lock_cloud(&mgr);
            let result = mgr.upload(&id, Path::new(&local_path), &remote);
            let done_msg = if result.is_ok() {
                format!("素材 {file_name} 已上传到云存储")
            } else {
                format!(
                    "素材 {file_name} 上传失败：{}",
                    result
                        .as_ref()
                        .err()
                        .map(|e| e.to_string())
                        .unwrap_or_else(|| "未知错误".to_string())
                )
            };
            let _ = progress_tx.send((done_msg, 1.0));
            event::emit(event::Event::cloud(
                cloud_event::Event::UploadMaterialResult {
                    ok: result.is_ok(),
                    error: result.err().map(|e| e.to_string()),
                },
            ));
        });
    }

    /// 注入素材上传结果
    pub(super) fn apply_cloud_upload_result(&mut self, ok: bool, error: Option<String>) {
        // 云上传结束（与保存到云共用串行标志）
        self.cloud_saving
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                state.notice = Some("素材已上传到云存储".to_string());
                false
            } else {
                state.notice = Some(Self::cloud_error_text("素材上传失败", error.as_deref()));
                true
            }
        };
        if failed {
            self.report_cloud_error("云存储连接异常", error.as_deref());
        } else {
            // 刷新当前目录列表（显示新上传的素材文件）
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

    // ── 保存结果注入 ──

    /// 注入保存结果
    pub(super) fn apply_cloud_save_result(&mut self, ok: bool, error: Option<String>) {
        // 云上传结束（手动保存到云 / Ctrl+S 自动回传共用同一结果事件）
        self.cloud_saving
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let failed = {
            let state = self.window_state.window.ui_mut().cloud_state_mut();
            state.busy = false;
            if ok {
                state.notice = Some("已保存到云存储".to_string());
                false
            } else {
                state.notice = Some(Self::cloud_error_text("保存失败", error.as_deref()));
                true
            }
        };
        if failed {
            self.report_cloud_error("云存储连接异常", error.as_deref());
        } else {
            self.sync_cloud_to_dialogs();
        }
    }
}

/// 覆盖上传的临时远程路径：原路径追加 `.saving` 后缀（同目录，重命名不跨目录）
///
/// 示例：`/dir/project.lmpj` → `/dir/project.lmpj.saving`；`project.lmpj` → `project.lmpj.saving`
fn overwrite_tmp_path(remote_path: &str) -> String {
    format!("{remote_path}.saving")
}

#[cfg(test)]
mod tests {
    use super::overwrite_tmp_path;

    /// 临时名生成：保留目录结构，仅追加后缀
    #[test]
    fn test_overwrite_tmp_path() {
        assert_eq!(
            overwrite_tmp_path("/dir/project.lmpj"),
            "/dir/project.lmpj.saving"
        );
        assert_eq!(overwrite_tmp_path("project.lmpj"), "project.lmpj.saving");
        assert_eq!(overwrite_tmp_path("/"), "/.saving");
    }
}
