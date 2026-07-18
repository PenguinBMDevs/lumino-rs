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
                // 阻塞加载刚保存的 MIDI 文件以获取完整文档
                if let Some(ref source) = self.midi_state.current_midi_source.clone() {
                    match futures::executor::block_on(lumino_midi_loader::loader::load_midi(
                        source.clone(),
                    )) {
                        Ok(parsed) => {
                            self.midi_state.current_midi = Some(Arc::new(parsed));
                        }
                        Err(e) => {
                            tracing::error!("自动保存后加载 MIDI 失败: {}", e);
                            return;
                        }
                    }
                } else {
                    return;
                }
            }
        }

        let Some(parsed_midi) = self.midi_state.current_midi.as_ref() else {
            tracing::warn!("没有加载的 MIDI 文件，无法导出工程");
            return;
        };

        let Some(document) = parsed_midi.document.as_ref() else {
            tracing::warn!("MidiDocument 已释放，无法导出工程");
            return;
        };

        let file_stem = get_file_stem(Path::new(&parsed_midi.info.path));

        let Some(save_path) = rfd::FileDialog::new()
            .set_file_name(format!("{file_stem}.lmpj"))
            .pick_folder()
        else {
            return;
        };

        let project = lumino_export::LuminoProject::from_midi_document(document);
        let cb = self.window_state.progress_cb.clone();

        tokio::spawn(async move {
            cb("准备导出工程", 0.0);
            cb("正在导出工程", 0.3);

            let path_clone = save_path.clone();
            match tokio::task::spawn_blocking(move || {
                lumino_export::project::save::save_to_folder(&project, path_clone)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("工程导出成功", 1.0);
                    tracing::info!("工程导出成功: {:?}", save_path);
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
