//! Runner 文件菜单：工程导出

use std::path::Path;
use std::sync::Arc;

use lumino_core::{tr, trf};

use crate::runner::RunnerInner;

use super::helpers::get_file_stem;

impl RunnerInner {
    /// 导出前检查：缺 EOT 的轨需要用户确认（Ok=继续导出，Cancel=中止）。
    ///
    /// 语义与导入 toast 区分：只有「缺 EOT」会打断导出；「EOT 之后仍有事件」
    /// 已在导入时提示过，导出不重复打扰。
    pub(super) fn confirm_missing_eot_before_export(&self) -> bool {
        let missing = self.midi_state.timing_report.tracks_missing_eot();
        if missing == 0 {
            return true;
        }
        #[cfg(not(test))]
        {
            let result = lumino_platform::dialog::MessageDialog::new()
                .set_title(tr!("导出前检查"))
                .set_description(trf!(
                    "{n} 条轨缺少 End-of-Track 标记，部分播放器可能无法正确识别结尾。仍要导出吗？",
                    n = missing
                ))
                .set_level(lumino_platform::dialog::MessageLevel::Warning)
                .set_buttons(lumino_platform::dialog::MessageButtons::OkCancel)
                .show();
            matches!(result, lumino_platform::dialog::MessageDialogResult::Ok)
        }
        #[cfg(test)]
        {
            let _ = missing;
            true
        }
    }

    /// 导出为单文件（统一入口：与"保存"共享相同格式选择对话框）
    pub(super) fn handle_export_project_archive(&mut self) {
        // 注意：该入口与「保存」共用，可能写 .lmpj 工程；缺 EOT 的提示只放在
        // 明确的「导出 MIDI / 导出工程文件夹」入口，避免保存工程时弹窗打扰。
        self.handle_save_single_file();
    }

    /// 导出工程为文件夹
    ///
    /// **自动提交编辑**：进入导出流程前调用 `commit_current_edit()`，
    /// 保证导出的数据包含用户当前正在编辑的音符。
    pub(super) fn handle_export_project_folder(&mut self) {
        if !self.confirm_missing_eot_before_export() {
            return;
        }
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
                            // REND-002：自动保存重载同一文档，按文档端口布局强制重建
                            // 合成管线（清通道模态；失败保持旧布局）。
                            self.midi_state.midi.apply_midi_port_layout(doc.max_port());
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

        // DEBT-23：主线程只做轻量快照（clone document，块级 COW），组包下沉到
        // spawn_blocking。复用与本地保存/自动保存/云保存同一口径 LmpjSnapshot，
        // 元数据装配归一到一处。file_stem 作 project_name（原导出未设 name=默认
        // Untitled，快照补成文件名，与本地保存对齐、正向补全，不回退）。
        // 快照必须在弹对话框前完成（对话框 borrowing &mut self，且快照时点=导出触发点）。
        let Some(snapshot) = self.snapshot_lmpj_project(Path::new(&format!("{file_stem}.lmpj")))
        else {
            tracing::warn!("没有加载的 MIDI 文件，无法导出工程");
            return;
        };

        let Some(entry_path) = lumino_platform::dialog::FileDialog::new()
            .set_file_name(format!("{file_stem}.lmpj"))
            .add_filter(tr!("Lumino 工程入口"), &["lmpj"])
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
            cb(tr!("准备导出工程"), 0.0);
            cb(tr!("正在导出工程"), 0.3);

            let path_clone = entry_path.clone();
            match tokio::task::spawn_blocking(move || {
                // DEBT-23：组包在后台线程完成（主线程只做了 COW 快照）。
                // spawn_blocking 的 panic 由下方 Err(e) 分支收口（JoinError），
                // 本路径无 saving 标志，不会像云保存裸 spawn 那样留下未清标志。
                let project = snapshot.into_project();
                lumino_export::save_project_to_folder_with_entry(&project, path_clone, key_count)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb(tr!("工程导出成功"), 1.0);
                    tracing::info!("工程导出成功: {:?}", entry_path);
                }
                Ok(Err(e)) => {
                    let msg = trf!("导出失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = trf!("导出任务失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }
}
