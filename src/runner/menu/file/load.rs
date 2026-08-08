//! Runner 文件菜单：加载与导入
//!
//! 支持常规 MIDI/LMPJ 文件和压缩包文件的自动解压加载。
//! 压缩包中：
//! - 0 个 MIDI 文件 → 弹出错误提示
//! - 1 个 MIDI 文件 → 自动解压并加载
//! - 多个 MIDI 文件 → 弹出选择对话框（当前自动选择第一个，待完善）

use std::path::{Path, PathBuf};

use lumino_midi_loader::loader::archive_loading::{
    ArchiveLoadResult, extract_entry_with_tempdir, scan_file_for_midi,
};

use crate::runner::{RunnerInner, async_helper::run_async_task};

/// 判断路径是否为 LMPJ 工程文件
fn is_lmpj_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lmpj"))
}

impl RunnerInner {
    /// 打开文件（支持 MIDI 文件和压缩包自动解压）
    pub(super) fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_state.file_handler.handle_open_file() else {
            return;
        };

        // 解锁调色板，新 MIDI 加载后 MidiParsed 会重新锁定
        lumino_extras::palette::unlock_palette();

        tracing::info!("用户选择了文件：{:?}", path);
        self.handle_midi_or_archive(path);
    }

    /// 处理 MIDI 文件或压缩包文件的加载
    fn handle_midi_or_archive(&self, path: PathBuf) {
        match scan_file_for_midi(&path) {
            ArchiveLoadResult::NotArchive => {
                // 常规 MIDI / LMPJ 文件，直接加载
                tracing::info!("常规 MIDI 文件，直接加载：{:?}", path);
                self.load_midi_file(path);
            }
            ArchiveLoadResult::NoMidiFiles => {
                // 压缩包但内部没有 MIDI 文件
                tracing::warn!("压缩包中未找到 MIDI 文件：{:?}", path);
                self.show_error_dialog(
                    "本文件不支持加载，请检查文件格式！\n\n支持的格式：\n- MIDI 文件 (.mid / .midi)\n- Lumino 项目文件 (.lmpj)\n- 包含上述文件的压缩包 (.zip / .rar / .7z 等)",
                );
            }
            ArchiveLoadResult::SingleMidiFile(extracted_path) => {
                // 压缩包中只有一个 MIDI 文件，自动解压并加载
                tracing::info!(
                    "压缩包自动解压成功，加载 MIDI：{:?}（来自：{:?}）",
                    extracted_path,
                    path
                );
                self.load_midi_file(extracted_path);
            }
            ArchiveLoadResult::MultipleMidiFiles(midi_list) => {
                // 多个 MIDI 文件，显示列表供用户选择
                tracing::info!(
                    "压缩包中发现多个 MIDI 文件（{} 个），提示用户选择",
                    midi_list.len()
                );
                self.handle_multiple_midi_in_archive(&path, midi_list);
            }
        }
    }

    /// 处理压缩包中有多个 MIDI 文件的情况
    ///
    /// TODO: 当前实现弹窗列出文件并自动选择第一个加载。
    /// 未来应实现一个真正的选择对话框，让用户从中选择要加载的文件。
    fn handle_multiple_midi_in_archive(&self, archive_path: &PathBuf, midi_list: Vec<String>) {
        // 构建文件列表字符串
        let file_list: String = midi_list
            .iter()
            .enumerate()
            .map(|(i, name)| format!("{}. {}", i + 1, name))
            .collect::<Vec<_>>()
            .join("\n");

        // 自动选择第一个 MIDI 文件加载
        if let Some(first_midi) = midi_list.first() {
            match extract_entry_with_tempdir(archive_path, first_midi) {
                Ok((temp_dir, extracted_path)) => {
                    // 使用 keep() 阻止 TempDir 自动删除，确保 load_midi_file
                    // 的异步任务能读到文件。临时文件会留在 OS 临时目录中。
                    // TODO: 更好的做法是将 TempDir 存入窗口状态管理其生命周期
                    let _ = temp_dir.keep();
                    tracing::info!(
                        "自动选择并加载第一个 MIDI 文件：{}（来自：{:?}）",
                        first_midi,
                        archive_path
                    );
                    self.show_info_dialog(&format!(
                        "该压缩包中包含多个 MIDI 文件：\n\n{file_list}\n\n已自动加载第一个文件：{first_midi}\n如需加载其他文件，请直接打开对应的 MIDI 文件。"
                    ));
                    self.load_midi_file(extracted_path);
                }
                Err(e) => {
                    tracing::error!("提取 MIDI 文件失败：{e}");
                    self.show_error_dialog(&format!(
                        "解压 MIDI 文件失败：{e}\n\n请尝试手动解压后加载。"
                    ));
                }
            }
        }
    }

    /// 导入文件（支持 MIDI 文件和压缩包自动解压）
    pub(super) fn handle_import_files(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                crate::constants::filters::MUSIC_AND_ARCHIVE.0,
                crate::constants::filters::MUSIC_AND_ARCHIVE.1,
            )
            .add_filter(
                crate::constants::filters::MUSIC_FILES.0,
                crate::constants::filters::MUSIC_FILES.1,
            )
            .add_filter(
                crate::constants::filters::MIDI_FILES.0,
                crate::constants::filters::MIDI_FILES.1,
            )
            .add_filter(
                crate::constants::filters::LUMINO_PROJECT.0,
                crate::constants::filters::LUMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::ARCHIVE_FILES.0,
                crate::constants::filters::ARCHIVE_FILES.1,
            )
            .add_filter(
                crate::constants::filters::ALL_FILES.0,
                crate::constants::filters::ALL_FILES.1,
            )
            .pick_file()
        else {
            return;
        };

        // 解锁调色板，新 MIDI 加载后 MidiParsed 会重新锁定
        lumino_extras::palette::unlock_palette();

        tracing::info!("开始导入文件：{:?}", path);
        self.handle_midi_or_archive(path);
    }

    /// 显示错误对话框
    fn show_error_dialog(&self, message: &str) {
        #[cfg(not(test))]
        {
            let _ = rfd::MessageDialog::new()
                .set_title("加载失败")
                .set_description(message)
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        #[cfg(test)]
        {
            let _ = message;
        }
        tracing::error!("{message}");
    }

    /// 显示信息对话框
    fn show_info_dialog(&self, message: &str) {
        #[cfg(not(test))]
        {
            let _ = rfd::MessageDialog::new()
                .set_title("提示")
                .set_description(message)
                .set_level(rfd::MessageLevel::Info)
                .show();
        }
        #[cfg(test)]
        {
            let _ = message;
        }
        tracing::info!("{message}");
    }

    /// 加载 MIDI 文件（后台异步加载）
    pub(crate) fn load_midi_file(&self, path: PathBuf) {
        if is_lmpj_path(&path) {
            self.load_lmpj_project(path);
            return;
        }
        if lumino_export::is_material_path(&path) {
            self.load_material_project(path);
            return;
        }

        tracing::info!("开始后台加载 MIDI 文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_midi_loader::loader::load_parsed_midi(path, Some(&progress_cb)),
                |parsed| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parsed(std::sync::Arc::new(
                            parsed,
                        )),
                    )
                },
                |e| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::midi_parse_error(e),
                    )
                },
            )
            .await;
        });
    }

    /// 加载素材文件（.lmmaterial，后台异步加载）
    ///
    /// 素材内部使用 lmpj 归档形式存储，加载后通过 metadata 的
    /// `[material]` 段分辨素材文件与标准工程文件（非素材则拒绝加载）。
    fn load_material_project(&self, path: PathBuf) {
        tracing::info!("开始后台加载素材文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            progress_cb("正在加载素材", 0.3);
            let path_for_blocking = path.clone();
            let load_result = tokio::task::spawn_blocking(move || {
                let project = lumino_export::load_project(&path_for_blocking)?;
                // 从 metadata 分辨素材/工程：非素材文件拒绝按素材加载
                if !project.metadata.is_material_file() {
                    return Err(lumino_export::ExportError::FileFormat(format!(
                        "{} 不是素材文件（.lmmaterial）",
                        path_for_blocking.display()
                    )));
                }
                lumino_export::project_to_parsed_midi(&project, path_for_blocking)
            })
            .await;

            match load_result {
                Ok(Ok(parsed)) => {
                    progress_cb("素材加载成功", 1.0);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParsed(std::sync::Arc::new(
                            parsed,
                        )),
                    ));
                }
                Ok(Err(e)) => {
                    let msg = format!("加载素材失败: {e}");
                    progress_cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParseError(msg),
                    ));
                }
                Err(e) => {
                    let msg = format!("加载素材任务失败: {e}");
                    progress_cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParseError(msg),
                    ));
                }
            }
        });
    }

    /// 加载 LMPJ 工程文件（新格式或旧版兼容）
    fn load_lmpj_project(&self, path: PathBuf) {
        tracing::info!("开始后台加载 LMPJ 工程：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            progress_cb("正在加载 LMPJ 工程", 0.3);
            let path_for_blocking = path.clone();
            let load_result = tokio::task::spawn_blocking(move || {
                let project = lumino_export::load_project(&path_for_blocking)?;
                lumino_export::project_to_parsed_midi(&project, path_for_blocking)
            })
            .await;

            match load_result {
                Ok(Ok(parsed)) => {
                    progress_cb("工程加载成功", 1.0);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParsed(std::sync::Arc::new(
                            parsed,
                        )),
                    ));
                }
                Ok(Err(e)) => {
                    let msg = format!("加载 LMPJ 工程失败: {e}");
                    progress_cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParseError(msg),
                    ));
                }
                Err(e) => {
                    let msg = format!("加载 LMPJ 工程任务失败: {e}");
                    progress_cb(&msg, 1.0);
                    tracing::error!("{}", msg);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::MidiParseError(msg),
                    ));
                }
            }
        });
    }

    /// 将 MIDI 数据导入到编辑器
    ///
    /// 2026-08 单一权威源改造：`parsed` 以所有权传入，`MidiDocument` 在
    /// midi_handler 内零拷贝拆出（Arc::try_unwrap）并移入 UI 的 EditorData.document。
    pub(super) fn import_midi_to_editor(&mut self, parsed: lumino_midi_loader::ParsedMidi) {
        {
            let ui = self.window_state.window.ui_mut();
            self.midi_state
                .midi_handler
                .import_midi_to_editor(ui, parsed);
        }

        // MIDI 导入后，为播放管理器绑定一个独立的 MIDI 输出连接
        if let Some(output) = self.midi_state.midi.create_additional_output() {
            self.window_state
                .window
                .ui_mut()
                .set_playback_midi_output(output);
            tracing::info!("Playback MIDI output connected");
        } else {
            tracing::warn!("Failed to create playback MIDI output connection");
        }
    }
}
