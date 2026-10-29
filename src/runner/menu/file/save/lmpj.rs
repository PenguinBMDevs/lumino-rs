//! LMPJ 保存：主线程快照与后台组包/写盘。
//!
//! DEBT-10 #240：`LuminoProject::from_midi_document` 会为每轨分配 compact
//! 事件并排序，大工程下是 UI 线程的秒级同步段。这里把保存拆成两段：
//! 1. 主线程只克隆 `MidiDocument`（块级 COW，成本与轨数/块数相关）、形状和元数据；
//! 2. 组包与归档写盘整体进入 `spawn_blocking`。
//!
//! REND-019 的失败兜底仍使用 [`RunnerInner::build_lmpj_project`] 的同步口径，
//! 因为它要求在进入失败对话框前完成落盘（红线 9），不能改成异步。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use lumino_core::{tr, trf};
use lumino_midi_loader::MidiDocument;

use crate::runner::RunnerInner;

/// 保存快照：只包含构建 `LuminoProject` 所需的数据。
///
/// `MidiDocument::clone` 为手写实现，音符块/控制事件块共享底层 `Arc`，
/// 不复制逐音符数据；快照之后的编辑不会进入本次保存。
struct LmpjSnapshot {
    document: MidiDocument,
    shapes: Vec<lumino_core::shape::ShapeModel>,
    author: String,
    copyright: String,
    working_time_seconds: f64,
    project_name: String,
}

impl LmpjSnapshot {
    /// 在后台线程完成工程组包。
    fn into_project(self) -> lumino_export::LuminoProject {
        let mut project = lumino_export::LuminoProject::from_midi_document(&self.document);
        project.shapes = self.shapes;
        project.metadata.project.name = self.project_name;
        project.metadata.project.author = self.author;
        project.metadata.project.copyright = self.copyright;
        project.set_working_time_seconds(self.working_time_seconds);
        project
    }
}

impl RunnerInner {
    /// 构建待写入的 LMPJ 工程（同步口径）。
    ///
    /// 供 REND-019 的 `auto_save_before_failed` 使用：该路径必须在进入
    /// 失败兜底对话框前同步完成落盘，不能改后台。普通手动保存走
    /// [`Self::save_lmpj_project_to`] 的快照 + 后台路径。
    pub(crate) fn build_lmpj_project(
        &self,
        save_path: &Path,
    ) -> Option<lumino_export::LuminoProject> {
        self.snapshot_lmpj_project(save_path)
            .map(LmpjSnapshot::into_project)
    }

    /// 主线程轻量快照：不组包、不排序、不分配逐音符 compact 事件。
    fn snapshot_lmpj_project(&self, save_path: &Path) -> Option<LmpjSnapshot> {
        let (document, shapes, author, copyright) = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            // 单一权威源：无文档 = 没有可保存的音符。
            let document = data.document.clone()?;
            let shapes = ui.root().editor.shapes_snapshot();
            let author = ui.get_project_author();
            let copyright = ui.get_project_copyright();
            (document, shapes, author, copyright)
        };

        let project_name = save_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());

        Some(LmpjSnapshot {
            document,
            shapes,
            author,
            copyright,
            working_time_seconds: self.session_tracker.current_editing_secs(),
            project_name,
        })
    }

    /// 周期自动保存：快照后后台组包并写入独立文件。
    ///
    /// 不设置 `saving` 标志、不发进度事件、不触碰用户 `.lmpj` 源文件。
    /// `autosave_in_flight` 在快照前置位，防上轮写入与本轮快照重叠。
    pub(crate) fn spawn_autosave_lmpj(&self, path: PathBuf) -> bool {
        if self.autosave_in_flight.swap(true, Ordering::SeqCst) {
            return false;
        }

        let Some(snapshot) = self.snapshot_lmpj_project(&path) else {
            self.autosave_in_flight.store(false, Ordering::SeqCst);
            return false;
        };

        let in_flight = Arc::clone(&self.autosave_in_flight);
        let cleanup_pending = Arc::clone(&self.autosave_cleanup_pending);
        let failed = Arc::clone(&self.autosave_failed);
        let path_for_log = path.clone();
        tokio::spawn(async move {
            let path_for_task = path.clone();
            let result = tokio::task::spawn_blocking(move || {
                if let Some(parent) = path_for_task.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let project = snapshot.into_project();
                lumino_export::save_to_archive(&project, &path_for_task)
            })
            .await;

            in_flight.store(false, Ordering::SeqCst);
            if cleanup_pending.swap(false, Ordering::SeqCst) {
                let _ = std::fs::remove_file(&path_for_log);
            }

            match result {
                Ok(Ok(())) => tracing::info!("周期自动保存完成: {:?}", path_for_log),
                Ok(Err(e)) => {
                    failed.store(true, Ordering::SeqCst);
                    tracing::warn!("周期自动保存写入失败 {:?}: {e}", path_for_log);
                }
                Err(e) => {
                    failed.store(true, Ordering::SeqCst);
                    tracing::warn!("周期自动保存任务失败 {:?}: {e}", path_for_log);
                }
            }
        });

        true
    }

    /// 保存为 LMPJ 文件（快照后后台组包 + 归档写盘）。
    ///
    /// 返回 `true` 表示保存任务已真正启动；`false` 表示未启动
    /// （保存/云上传进行中被拒绝，或无文档可写）。
    pub(super) fn save_lmpj_project_to(&mut self, save_path: PathBuf) -> bool {
        // 串行限制：保存/云上传进行中，新保存请求直接拒绝
        // （上传完成后用户再按 Ctrl+S 即可，不排队不补传）
        if self.is_saving() || self.is_cloud_saving() {
            tracing::debug!("保存或云上传进行中，忽略重复的保存请求");
            return false;
        }

        let Some(snapshot) = self.snapshot_lmpj_project(&save_path) else {
            tracing::warn!("没有加载的 MIDI 文件且没有编辑器内容，无法保存 LMPJ 格式");
            return false;
        };

        // 标记保存进行中（保存期间禁止关闭软件）
        self.saving.store(true, Ordering::SeqCst);
        let saving = Arc::clone(&self.saving);
        let cb = Arc::clone(&self.window_state.progress_cb);
        tokio::spawn(async move {
            cb(tr!("正在保存 LMPJ 文件"), 0.3);
            let save_path_for_task = save_path.clone();
            let result = tokio::task::spawn_blocking(move || {
                // 组包与写盘都在阻塞线程：UI 线程只承担上面的快照。
                let project = snapshot.into_project();
                lumino_export::save_to_archive(&project, &save_path_for_task)
            })
            .await;

            // 无论成败均清除保存标志（解除关闭限制）
            saving.store(false, Ordering::SeqCst);

            match result {
                Ok(Ok(())) => {
                    cb(tr!("工程保存成功"), 1.0);
                    tracing::info!("工程保存成功: {:?}", save_path);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_completed(save_path),
                    ));
                }
                Ok(Err(e)) => {
                    let msg = trf!("保存失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_failed(msg),
                    ));
                }
                Err(e) => {
                    let msg = trf!("保存任务失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_failed(msg),
                    ));
                }
            }
        });

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_midi_loader::{ChunkedList, TrackManager};

    fn empty_document() -> MidiDocument {
        MidiDocument {
            notes: Vec::new(),
            tempo_changes: Vec::new(),
            time_signatures: Vec::new(),
            key_signatures: Vec::new(),
            control_events: ChunkedList::new(),
            lyrics: Vec::new(),
            markers: Vec::new(),
            text_events: Vec::new(),
            sys_ex: Vec::new(),
            track_names: Vec::new(),
            total_ticks: 0,
            track_count: 0,
            tracks: TrackManager::new(0),
            division: 480,
            track_ports: Vec::new(),
            track_max_end_ticks: Vec::new(),
        }
    }

    #[test]
    fn test_snapshot_into_project_keeps_metadata() {
        let snapshot = LmpjSnapshot {
            document: empty_document(),
            shapes: Vec::new(),
            author: "Author".into(),
            copyright: "Copyright".into(),
            working_time_seconds: 42.0,
            project_name: "Project".into(),
        };

        let project = snapshot.into_project();

        assert_eq!(project.metadata.project.name, "Project");
        assert_eq!(project.metadata.project.author, "Author");
        assert_eq!(project.metadata.project.copyright, "Copyright");
        assert_eq!(
            project
                .metadata
                .stats
                .as_ref()
                .and_then(|stats| stats.working_time_seconds),
            Some(42),
            "累计创作时长应取快照时刻"
        );
    }
}
