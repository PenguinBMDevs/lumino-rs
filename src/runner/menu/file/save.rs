//! Runner 文件菜单：保存

use std::path::{Path, PathBuf};

use crate::runner::RunnerInner;

use super::editor_midi;
use super::helpers::{get_file_extension, get_file_stem};

impl RunnerInner {
    /// 将编辑器内容保存为新的 MIDI 文件
    ///
    /// 用于从空白状态（未加载任何 MIDI/DMS 文件）保存用户编辑的音符。
    /// 设置 `current_midi_source` 并触发异步后台加载以填充 `current_midi`。
    pub(super) fn save_editor_as_midi_file(&mut self) -> Option<PathBuf> {
        let has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };
        if !has_notes {
            return None;
        }

        let save_path = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .set_file_name("untitled.mid")
            .save_file()?;

        let export_data = editor_midi::build_midi_export_data_from_editor(self, true)?;

        // 写入 MIDI 文件
        if let Err(e) = lumino_export::export_midi(&save_path, &export_data) {
            tracing::error!("保存新项目失败: {}", e);
            return None;
        }

        // 设置源路径并触发后台加载（异步填充 current_midi）
        self.midi_state.current_midi_source = Some(save_path.clone());
        self.load_midi_file(save_path.clone());

        tracing::info!("新项目已保存为 MIDI 文件: {:?}", save_path);
        Some(save_path)
    }

    /// 保存文件（统一入口：显示格式选择对话框，支持 lmpj/mid/midi）
    pub(super) fn handle_save_file(&mut self) {
        self.handle_save_single_file();
    }

    /// 统一保存/导出为单文件：显示格式选择对话框，支持 lmpj/mid/midi
    ///
    /// **自动提交编辑**：进入对话框前调用 `commit_current_edit()`，
    /// 保证保存的数据包含用户当前正在编辑（ghost 拖动/绘制/调整大小）的音符。
    pub(super) fn handle_save_single_file(&mut self) {
        // 自动提交当前编辑（ghost 方案：松手即提交）
        let committed = self
            .window_state
            .window
            .ui_mut()
            .root_mut()
            .editor
            .commit_current_edit();
        if committed {
            tracing::debug!("保存前自动提交编辑");
        }

        let file_stem = self
            .midi_state
            .current_midi_source
            .as_ref()
            .or_else(|| self.midi_state.current_midi.as_ref().map(|m| &m.info.path))
            .map(|p| get_file_stem(Path::new(p)))
            .unwrap_or_else(|| "untitled".to_string());

        let Some(save_path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::LUMINO_PROJECT.0,
                crate::constants::filters::LUMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .set_file_name(format!("{file_stem}.lmpj"))
            .save_file()
        else {
            return;
        };

        let extension = get_file_extension(&save_path);

        match extension.as_str() {
            "lmpj" => self.save_as_lmpj_project(save_path),
            "mid" | "midi" => self.save_as_midi_with_edits(save_path),
            _ => tracing::warn!("不支持的保存格式: {}", extension),
        }
    }

    /// 保存为 LMPJ 文件（默认使用新格式：按音轨拆分 + 归档）
    ///
    /// 2026-08 单一权威源：优先借用 UI 的 `MidiDocument`（零拷贝）构建 LuminoProject；
    /// 无文档时从编辑器音符重建。保证工程自包含——原始文件可删除后仍能完整加载。
    fn save_as_lmpj_project(&mut self, save_path: PathBuf) {
        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            if let Some(doc) = data.document.as_ref() {
                let mut project = lumino_export::LuminoProject::from_midi_document(doc);
                // 使用实际保存路径的文件名作为工程名
                if let Some(stem) = save_path.file_stem() {
                    project.metadata.project.name = stem.to_string_lossy().into_owned();
                }
                Some(project)
            } else {
                // 无文档时从编辑器音符重建（不再深拷贝 runner 侧 document）
                editor_midi::build_editor_midi_document(self).map(|doc| {
                    let mut project = lumino_export::LuminoProject::from_midi_document(&doc);
                    if let Some(stem) = save_path.file_stem() {
                        project.metadata.project.name = stem.to_string_lossy().into_owned();
                    }
                    project
                })
            }
        };

        let Some(project) = project else {
            tracing::warn!("没有加载的 MIDI 文件且没有编辑器内容，无法保存 LMPJ 格式");
            return;
        };

        let cb = self.window_state.progress_cb.clone();
        let save_path2 = save_path.clone();
        tokio::spawn(async move {
            cb("正在保存 LMPJ 文件", 0.3);
            match tokio::task::spawn_blocking(move || {
                lumino_export::save_to_archive(&project, &save_path2)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("工程保存成功", 1.0);
                    tracing::info!("工程保存成功: {:?}", save_path);
                }
                Ok(Err(e)) => {
                    let msg = format!("保存失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("保存任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }

    /// 保存为 MIDI（包含编辑器编辑 + 源文件的 PC/CC 事件）
    fn save_as_midi_with_edits(&mut self, save_path: PathBuf) {
        let editor_has_notes = {
            let ui = self.window_state.window.ui();
            ui.get_editor_note_count() > 0
        };

        if editor_has_notes {
            if let Some(export_data) = editor_midi::build_midi_export_data_from_editor(self, true) {
                let save_path2 = save_path.clone();
                tokio::spawn(async move {
                    match tokio::task::spawn_blocking(move || {
                        lumino_export::export_midi(&save_path2, &export_data)
                    })
                    .await
                    {
                        Ok(Ok(())) => tracing::info!("MIDI 保存成功: {:?}", save_path),
                        Ok(Err(e)) => tracing::error!("MIDI 保存失败: {}", e),
                        Err(e) => tracing::error!("MIDI 保存任务失败: {}", e),
                    }
                });
            }
            return;
        }

        // 无编辑器编辑，从已有源路径/文档导出
        let file_service = self.file_state.file_service.clone();
        if let Some(source_path) = &self.midi_state.current_midi_source {
            let source = source_path.clone();
            tokio::spawn(async move {
                let _ = file_service.save_as_midi(source, save_path).await;
            });
        } else if let Some(parsed_midi) = &self.midi_state.current_midi {
            let source = parsed_midi.info.path.clone();
            tokio::spawn(async move {
                let _ = file_service.save_as_midi(source, save_path).await;
            });
        }
    }
}
