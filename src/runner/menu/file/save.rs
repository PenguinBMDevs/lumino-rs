//! Runner 文件菜单：保存

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::runner::RunnerInner;

use super::editor_midi;
use super::helpers::{get_file_extension, get_file_stem};
use super::material;

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
        let Some(save_path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::LUMINO_MATERIAL.0,
                crate::constants::filters::LUMINO_MATERIAL.1,
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
            let mut project =
                material::build_material_project_from_selection(doc, &selected, &data.tempo_points);
            // 作者栏：素材文件继承工程设置对话框中填写的作者
            project.metadata.project.author = ui.get_project_author();
            project
        };

        // 后台写入素材文件（进度条）
        let cb = self.window_state.progress_cb.clone();
        let save_path2 = save_path.clone();
        let material_name2 = material_name.clone();
        tokio::spawn(async move {
            cb("正在导出素材", 0.3);
            match tokio::task::spawn_blocking(move || {
                lumino_export::save_material(&project, &material_name2, &save_path2)
            })
            .await
            {
                Ok(Ok(())) => {
                    cb("素材导出成功", 1.0);
                    tracing::info!("素材导出成功: {:?}（名称: {}）", save_path, material_name);
                }
                Ok(Err(e)) => {
                    let msg = format!("素材导出失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                }
                Err(e) => {
                    let msg = format!("素材导出任务失败: {e}");
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

    /// 保存文件（统一入口：Ctrl+S / 菜单保存）
    ///
    /// **智能分流**：
    /// - 已有 .lmpj 工程文件路径（`current_midi_source`）→ 直接覆盖保存原文件（无对话框）
    /// - 无工程路径（空白工程 / 从 .mid 打开 / 从未保存过）→ 弹出格式选择对话框
    pub(super) fn handle_save_file(&mut self) {
        // 串行限制：保存/云上传进行中，新保存请求直接拒绝
        // （上传完成后用户再按 Ctrl+S 即可，不排队不补传）
        if self.is_saving() || self.is_cloud_saving() {
            tracing::debug!("保存或云上传进行中，忽略重复的保存请求");
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
            tracing::debug!("保存前自动提交编辑");
        }

        // 已有 .lmpj 工程路径 → 覆盖保存原文件（无对话框）
        if let Some(path) = self.current_lmpj_source() {
            self.save_lmpj_project_to(path);
            return;
        }

        // 首次保存：弹对话框选择路径/格式
        self.handle_save_single_file();
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
            "lmpj" => self.save_lmpj_project_to(save_path),
            "mid" | "midi" => self.save_as_midi_with_edits(save_path),
            _ => tracing::warn!("不支持的保存格式: {}", extension),
        }
    }

    /// 保存为 LMPJ 文件（默认使用新格式：按音轨拆分 + 归档）
    ///
    /// 2026-08 单一权威源：优先借用 UI 的 `MidiDocument`（零拷贝）构建 LuminoProject；
    /// 无文档时从编辑器音符重建。保证工程自包含——原始文件可删除后仍能完整加载。
    ///
    /// 保存成功后（经 `SaveCompleted` 事件回主线程）：
    /// - 记录保存路径到 `current_midi_source`（后续 Ctrl+S 覆盖保存）
    /// - 底边栏显示"文件已经保存"（3 秒后自动恢复"就绪"）
    /// - 若文件来自云端，自动上传回云端原路径
    ///
    /// 保存期间 `saving` 标志置位：禁止关闭软件，关闭请求转为 `pending_close`
    /// 延迟处理，保存完成后自动退出。
    fn save_lmpj_project_to(&mut self, save_path: PathBuf) {
        // 串行限制：保存/云上传进行中，新保存请求直接拒绝
        // （上传完成后用户再按 Ctrl+S 即可，不排队不补传）
        if self.is_saving() || self.is_cloud_saving() {
            tracing::debug!("保存或云上传进行中，忽略重复的保存请求");
            return;
        }

        let project = {
            let ui = self.window_state.window.ui();
            let data = &ui.root().editor.editor_state.data;
            if let Some(doc) = data.document.as_ref() {
                let mut project = lumino_export::LuminoProject::from_midi_document(doc);
                // 用编辑器 tempo_points 覆盖 doc 中的原始 tempo：
                // doc.tempo_changes 是加载文件时的值，用户经工程设置/速度面板
                // 修改的 BPM 只写入 tempo_points，不回写 doc——不覆盖会保存旧值
                // （新工程保存后 tempo 丢失，回落到默认 120 BPM 的 BUG 根因）。
                project.apply_tempo_points(data.tempo_points.iter().map(|tp| (tp.tick, tp.bpm)));
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

        // 工程设置对话框中填写的作者写入工程元数据（.lmpj / metadata.toml）
        let mut project = project;
        let author = self.window_state.window.ui().get_project_author();
        project.metadata.project.author = author;

        // 标记保存进行中（保存期间禁止关闭软件）
        self.saving.store(true, Ordering::SeqCst);
        let saving = self.saving.clone();
        let cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            cb("正在保存 LMPJ 文件", 0.3);
            let save_path_for_task = save_path.clone();
            let result = tokio::task::spawn_blocking(move || {
                lumino_export::save_to_archive(&project, &save_path_for_task)
            })
            .await;

            // 无论成败均清除保存标志（解除关闭限制）
            saving.store(false, Ordering::SeqCst);

            match result {
                Ok(Ok(())) => {
                    cb("工程保存成功", 1.0);
                    tracing::info!("工程保存成功: {:?}", save_path);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_completed(save_path),
                    ));
                }
                Ok(Err(e)) => {
                    let msg = format!("保存失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_failed(msg),
                    ));
                }
                Err(e) => {
                    let msg = format!("保存任务失败: {e}");
                    cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::save_failed(msg),
                    ));
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
