//! Runner 文件菜单：工程导出

use std::path::Path;
use std::sync::Arc;

use crate::runner::RunnerInner;

use super::helpers::get_file_stem;

impl RunnerInner {
    /// 导出为单文件（统一入口：与"保存"共享相同格式选择对话框）
    pub(super) fn handle_export_project_archive(&mut self) {
        self.handle_save_single_file();
    }

    /// 导出工程为文件夹
    ///
    /// **自动提交编辑**：进入导出流程前调用 `commit_current_edit()`，
    /// 保证导出的数据包含用户当前正在编辑的音符。
    pub(super) fn handle_export_project_folder(&mut self) {
        // 自动提交当前编辑（ghost 方案：松手即提交）
        let committed = self
            .window_state
            .window
            .ui_mut()
            .root_mut()
            .editor
            .commit_current_edit();
        if committed {
            tracing::debug!("导出工程前自动提交编辑");
        }

        // 如果没有加载 MIDI 但有编辑器内容，先自动保存
        if self.midi_state.current_midi.is_none() && self.midi_state.current_midi_source.is_none() {
            let has_notes = {
                let ui = self.window_state.window.ui();
                ui.get_editor_note_count() > 0
            };
            if has_notes {
                tracing::info!("导出工程：自动保存新项目");
                if self.save_editor_as_midi_file().is_none() {
                    return; // 用户取消保存
                }
                // 阻塞加载刚保存的 MIDI 文件，并把 document 移入 UI（单一权威源）
                if let Some(ref source) = self.midi_state.current_midi_source.clone() {
                    // 看门狗在加载前确保已启动，并标记加载状态（导出自动重载同样是加载 MIDI）
                    lumino_diagnostics::memory_monitor::watchdog::spawn_watchdog();
                    lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(true);
                    match futures::executor::block_on(lumino_midi_loader::loader::load_midi(
                        source.clone(),
                    )) {
                        Ok(parsed) => {
                            lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(
                                false,
                            );
                            // Arc::try_unwrap 零拷贝拆出（自动保存路径上 Arc 唯一）
                            let Some(doc) =
                                parsed.document.and_then(|arc| Arc::try_unwrap(arc).ok())
                            else {
                                tracing::error!("自动保存后加载 MIDI 失败: 无 document");
                                return;
                            };
                            let ui = self.window_state.window.ui_mut();
                            ui.set_midi_document(doc);
                        }
                        Err(e) => {
                            lumino_diagnostics::memory_monitor::midi_guard::set_midi_load_active(
                                false,
                            );
                            tracing::error!("自动保存后加载 MIDI 失败: {}", e);
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
        }

        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .map(|p| get_file_stem(Path::new(p)))
            .unwrap_or_else(|| "untitled".to_string());

        // 2026-08 单一权威源：借用 UI 的 document（零拷贝）构建工程
        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            let Some(document) = data.document.as_ref() else {
                tracing::warn!("没有加载的 MIDI 文件，无法导出工程");
                return;
            };
            let mut project = lumino_export::LuminoProject::from_midi_document(document);
            // 用编辑器 tempo_points 覆盖 doc 的加载时原始 tempo，
            // 保证用户修改的 BPM（工程设置/速度面板）随导出持久化
            project.apply_tempo_points(data.tempo_points.iter().map(|tp| (tp.tick, tp.bpm)));
            // 累计创作时间随导出持久化（与本地保存一致）
            project.set_working_time_seconds(self.session_tracker.current_editing_secs());
            project
        };

        let Some(entry_path) = rfd::FileDialog::new()
            .set_file_name(format!("{file_stem}.lmpj"))
            .add_filter("Lumino 工程入口", &["lmpj"])
            .save_file()
        else {
            return;
        };

        let key_count = if self.window_state.storage.config.get().ui.enable_256key {
            256
        } else {
            128
        };
        let cb = Arc::clone(&self.window_state.progress_cb);

        tokio::spawn(async move {
            cb("准备导出工程", 0.0);
            cb("正在导出工程", 0.3);

            let path_clone = entry_path.clone();
            match tokio::task::spawn_blocking(move || {
                lumino_export::save_project_to_folder_with_entry(&project, path_clone, key_count)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("工程导出成功", 1.0);
                    tracing::info!("工程导出成功: {:?}", entry_path);
                }
                Ok(Err(e)) => {
                    let msg = format!("导出失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("导出任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }
}
