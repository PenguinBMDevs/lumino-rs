//! Runner 文件菜单：工程导出与音频导出

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
    pub(super) fn handle_export_project_folder(&mut self) {
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

    /// 处理音频导出
    pub(super) fn handle_audio_export(&mut self) {
        // 获取当前配置
        let config = self.window_state.storage.config.get();
        let soundfont_path = config.ui.soundfont_path.clone();

        // 检查是否有音色库
        if soundfont_path.is_empty() {
            tracing::warn!("没有设置音色库路径，无法导出音频");
            // TODO: 显示错误对话框
            return;
        }

        // 场景1: 没有打开 MIDI 文件
        if self.midi_state.current_midi.is_none()
            && self.midi_state.current_midi_source.is_none()
            && self.midi_state.current_dms.is_none()
        {
            // 检查工作区是否为脏（有编辑内容）
            let has_notes = {
                let ui = self.window_state.window.ui();
                ui.get_editor_note_count() > 0
            };

            if has_notes {
                // 工作区有内容但没有打开 MIDI，先自动保存为 MIDI 文件
                tracing::info!("工作区有内容但没有打开 MIDI，先保存再导出音频");
                if self.save_editor_as_midi_file().is_none() {
                    return; // 用户取消保存
                }
                // 保存成功后 current_midi_source 已被设置，继续后续逻辑
            } else {
                tracing::warn!("没有可导出的内容");
                return;
            }
        }

        // 场景2: 打开了 MIDI 文件
        if let Some(parsed_midi) = &self.midi_state.current_midi {
            let midi_path = parsed_midi.info.path.clone();
            let project_name = get_file_stem(&midi_path);

            // 检查是否有额外的编辑内容
            let ui = self.window_state.window.ui();
            let has_extra_edits = ui.has_notes_changed();

            let output_path = if has_extra_edits {
                // 有额外编辑，需要导出为新的 MIDI 文件
                let file_stem = get_file_stem(&midi_path);
                let output_dir = midi_path.parent().unwrap_or_else(|| Path::new("."));
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                output_dir
                    .join(format!("{}_export_{}.wav", file_stem, timestamp))
                    .to_string_lossy()
                    .to_string()
            } else {
                // 没有额外编辑，复用原路径（改为 .wav 扩展名）
                let mut output = midi_path.clone();
                output.set_extension("wav");
                output.to_string_lossy().to_string()
            };

            // 打开音频导出对话框
            self.window_state.dialog_manager.open_audio_export(
                project_name,
                midi_path.to_string_lossy().to_string(),
                soundfont_path,
                output_path,
            );
            return;
        }

        // 场景3: 打开了 DMS 文件
        if let Some(_parsed_dms) = &self.midi_state.current_dms {
            // DMS 文件需要先转换为 MIDI
            // TODO: 实现 DMS 到 MIDI 的转换
            tracing::info!("DMS 文件导出音频功能待实现");
            return;
        }

        // 场景4: 有源路径但没有完整文档
        if let Some(source_path) = &self.midi_state.current_midi_source {
            let project_name = get_file_stem(source_path);
            let output_path = {
                let mut output = source_path.clone();
                output.set_extension("wav");
                output.to_string_lossy().to_string()
            };

            // 打开音频导出对话框
            self.window_state.dialog_manager.open_audio_export(
                project_name,
                source_path.to_string_lossy().to_string(),
                soundfont_path,
                output_path,
            );
        }
    }
}
