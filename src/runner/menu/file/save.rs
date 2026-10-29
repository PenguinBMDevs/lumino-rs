//! Runner 文件菜单：保存

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumino_core::{tr, trf};

use crate::runner::RunnerInner;

use super::editor_midi;
use super::helpers::{get_file_extension, get_file_stem};
use super::material;

mod lmpj;

impl RunnerInner {
    /// 导出为素材（.lmmaterial）
    ///
    /// 前置条件：存在音符框选（卷帘选中音符 / 走带跨音轨框选），
    /// 菜单项在无框选时置灰禁用，此处做兜底校验。
    ///
    /// 素材内容 = 选中音符（跨轨）+ 工程级数据（tempo/拍号/自动化 CC/PC/弯音），
    /// 保存为带 `[material]` 元数据标记的单文件 LMPJ 归档；
    /// 素材名以保存对话框中设置的文件名为准（二次导出用新名字）。
    pub(super) fn handle_export_material(&mut self) {
        // 自动提交当前编辑（ghost 方案：松手即提交）
        let committed = self
            .window_state
            .window
            .ui_mut()
            .root_mut()
            .editor
            .commit_current_edit();
        if committed {
            tracing::debug!("导出素材前自动提交编辑");
        }

        // 提取选中音符（卷帘 selected_notes / 走带 arrange_selection 跨轨）
        let selected = {
            let ui = self.window_state.window.ui();
            ui.get_selected_notes()
        };
        if selected.is_empty() {
            tracing::warn!("导出为素材：没有选中的音符，已忽略");
            return;
        }

        // 保存对话框（仅支持 .lmmaterial）
        let Some(save_path) = lumino_platform::dialog::FileDialog::new()
            .add_filter(
                crate::constants::filters::lumino_material_filter(),
                crate::constants::filters::LUMINO_MATERIAL_EXTS,
            )
            .set_file_name("untitled.lmmaterial")
            .save_file()
        else {
            return;
        };

        let Some(material_name) = save_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
        else {
            return;
        };

        // 直接从 UI 文档构建素材工程（不走 MIDI 字节中转，避免截断与配对错乱）
        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            let Some(doc) = data.document.as_ref() else {
                tracing::error!("导出为素材：文档不可用");
                return;
            };
            let mut project = material::build_material_project_from_selection(doc, &selected);
            // 作者/版权：素材文件继承工程设置对话框中填写的作者与版权
            project.metadata.project.author = ui.get_project_author();
            project.metadata.project.copyright = ui.get_project_copyright();
            project
        };

        // 后台写入素材文件（进度条）
        let cb = Arc::clone(&self.window_state.progress_cb);
        let save_path2 = save_path.clone();
        let material_name2 = material_name.clone();
        tokio::spawn(async move {
            cb(tr!("正在导出素材"), 0.3);
            match tokio::task::spawn_blocking(move || {
                lumino_export::save_material(&project, &material_name2, &save_path2)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb(tr!("素材导出成功"), 1.0);
                    tracing::info!("素材导出成功: {:?}（名称: {}）", save_path, material_name);
                }
                Ok(Err(e)) => {
                    let msg = trf!("素材导出失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = trf!("素材导出任务失败: {e}", e = e);
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
            }
        });
    }

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

        let save_path = lumino_platform::dialog::FileDialog::new()
            .add_filter(
                crate::constants::filters::midi_files_filter(),
                crate::constants::filters::MIDI_FILES_EXTS,
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

    /// 保存文件（统一入口：Ctrl+S / 菜单保存）
    ///
    /// **智能分流**：
    /// - 已有 .lmpj 工程文件路径（`current_midi_source`）→ 直接覆盖保存原文件（无对话框）
    /// - 无工程路径（空白工程 / 从 .mid 打开 / 从未保存过）→ 弹出格式选择对话框
    ///
    /// 保存工程文件
    ///
    /// 返回 `true` 表示保存已真正启动（已选路径并进入异步写入），
    /// `false` 表示未启动（保存进行中被拒绝 / 用户取消保存对话框）。
    /// 供保存确认对话框在「保存」后判断是否需要等待 `handle_save_completed`
    /// 继续挂起的关闭动作——未启动则视为取消关闭，绝不强制关闭工程。
    pub(super) fn handle_save_file(&mut self) -> bool {
        // 串行限制：保存/云上传进行中，新保存请求直接拒绝
        // （上传完成后用户再按 Ctrl+S 即可，不排队不补传）
        if self.is_saving() || self.is_cloud_saving() {
            tracing::debug!("保存或云上传进行中，忽略重复的保存请求");
            return false;
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
            tracing::debug!("保存前自动提交编辑");
        }

        // 已有 .lmpj 工程路径 → 覆盖保存原文件（无对话框）
        // DEBT-01 #118：透传真实启动状态——未启动（并发拒绝 / 无内容）时调用方
        // 不能挂起等待 `handle_save_completed`，否则关闭动作永久卡死。
        if let Some(path) = self.current_lmpj_source() {
            return self.save_lmpj_project_to(path);
        }

        // 首次保存：弹对话框选择路径/格式
        self.handle_save_single_file()
    }

    /// 当前工程文件路径（仅 .lmpj，Ctrl+S 覆盖保存适用）
    fn current_lmpj_source(&self) -> Option<PathBuf> {
        self.midi_state
            .current_midi_source
            .as_ref()
            .filter(|p| get_file_extension(p) == "lmpj")
            .cloned()
    }

    /// 统一保存/导出为单文件：显示格式选择对话框，支持 lmpj/mid/midi
    ///
    /// **自动提交编辑**：进入对话框前调用 `commit_current_edit()`，
    /// 保证保存的数据包含用户当前正在编辑（ghost 拖动/绘制/调整大小）的音符。
    ///
    /// 返回 `true` 表示已选择路径并进入保存流程；`false` 表示用户取消对话框。
    pub(super) fn handle_save_single_file(&mut self) -> bool {
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

        let Some(save_path) = lumino_platform::dialog::FileDialog::new()
            .add_filter(
                crate::constants::filters::lumino_project_filter(),
                crate::constants::filters::LUMINO_PROJECT_EXTS,
            )
            .add_filter(
                crate::constants::filters::midi_files_filter(),
                crate::constants::filters::MIDI_FILES_EXTS,
            )
            .set_file_name(format!("{file_stem}.lmpj"))
            .save_file()
        else {
            // 用户取消保存对话框
            return false;
        };

        let extension = get_file_extension(&save_path);

        match extension.as_str() {
            "lmpj" => self.save_lmpj_project_to(save_path),
            "mid" | "midi" => {
                self.save_as_midi_with_edits(save_path);
                true
            }
            _ => {
                tracing::warn!("不支持的保存格式: {}", extension);
                false
            }
        }
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
