//! Runner 文件菜单：加载与导入

use std::path::PathBuf;
use std::sync::Arc;

use crate::runner::{RunnerInner, async_helper::run_async_task};

use super::helpers::get_file_extension;

impl RunnerInner {
    /// 打开文件
    pub(super) fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_state.file_handler.handle_open_file() else {
            return;
        };

        let extension = get_file_extension(&path);

        if extension == "dms" {
            self.load_dms_file(path);
            return;
        }

        // 检查文件大小，大文件弹出确认对话框
        const MEMORY_OPTIMIZE_THRESHOLD_MB: u64 = 100;
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = file_size as f64 / (1024.0 * 1024.0);

        if size_mb > MEMORY_OPTIMIZE_THRESHOLD_MB as f64 {
            let path_str = path.to_string_lossy().to_string();
            // 存储 pending 路径，等待对话框结果
            self.file_state.pending_load_path = Some(path);
            // 使用已有 dialog 窗口基础设施
            self.window_state
                .dialog_manager
                .open_load_confirm(path_str, size_mb);
        } else {
            tracing::info!("文件大小 {:.1}MB ≤ 阈值，标准模式加载", size_mb);
            self.load_midi_file(path);
        }
    }

    /// 加载 DMS 文件
    pub(super) fn load_dms_file(&self, path: PathBuf) {
        tracing::info!("开始后台加载 DMS 文件：{:?}", path);
        let progress_cb = self.window_state.progress_cb.clone();
        tokio::spawn(async move {
            run_async_task(
                lumino_midi_loader::loader::load_dms(path, Some(&progress_cb)),
                |parsed| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::dms_parsed(Arc::new(parsed)),
                    )
                },
                |e| {
                    lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::dms_parse_error(e),
                    )
                },
            )
            .await;
        });
    }

    /// 加载 MIDI 文件
    pub(crate) fn load_midi_file(&self, path: PathBuf) {
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

    /// 导入文件
    pub(super) fn handle_import_files(&mut self) {
        let Some(path) = rfd::FileDialog::new()
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
                crate::constants::filters::DOMINO_PROJECT.0,
                crate::constants::filters::DOMINO_PROJECT.1,
            )
            .add_filter(
                crate::constants::filters::ALL_FILES.0,
                crate::constants::filters::ALL_FILES.1,
            )
            .pick_file()
        else {
            return;
        };

        let extension = get_file_extension(&path);

        if extension == "dms" {
            tracing::info!("开始后台导入 DMS 文件：{:?}", path);
            let progress_cb = self.window_state.progress_cb.clone();
            tokio::spawn(async move {
                run_async_task(
                    lumino_midi_loader::loader::load_dms(path, Some(&progress_cb)),
                    |parsed| {
                        lumino_ui::event::Event::menu_file(
                            lumino_ui::event::menu::file::Event::dms_parsed(Arc::new(parsed)),
                        )
                    },
                    |e| {
                        lumino_ui::event::Event::menu_file(
                            lumino_ui::event::menu::file::Event::dms_parse_error(e),
                        )
                    },
                )
                .await;
            });
            return;
        }

        // 检查文件大小，大文件弹出确认对话框
        const MEMORY_OPTIMIZE_THRESHOLD_MB: u64 = 100;
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = file_size as f64 / (1024.0 * 1024.0);

        if size_mb > MEMORY_OPTIMIZE_THRESHOLD_MB as f64 {
            let path_str = path.to_string_lossy().to_string();
            self.file_state.pending_load_path = Some(path);
            self.window_state
                .dialog_manager
                .open_load_confirm(path_str, size_mb);
        } else {
            tracing::info!("导入文件 {:.1}MB ≤ 阈值，标准模式加载", size_mb);
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
    }

    /// 将 MIDI 数据导入到编辑器
    pub(super) fn import_midi_to_editor(&mut self, parsed: &lumino_midi_loader::ParsedMidi) {
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

    /// 将 DMS 数据导入到编辑器
    pub(super) fn import_dms_to_editor(&mut self, parsed: &Arc<lumino_midi_loader::ParsedDms>) {
        tracing::info!("[DMS导入] 开始将 DMS 导入编辑器: {:?}", parsed.info.path);

        // DMS 需要先转换为 MIDI 格式才能导入编辑器
        // 使用 load_parsed_midi_from_bytes 直接从字节加载，避免临时文件
        let path = parsed.info.path.clone();
        let track_count = parsed.info.track_count;

        tokio::spawn(async move {
            tracing::info!("[DMS导入] 步骤1: 导出 DMS 为 MIDI");
            match lumino_export::export_midi_from_dms_sync(&path) {
                Ok(midi_bytes) => {
                    tracing::info!(
                        "[DMS导入] 步骤2: DMS 转换为 MIDI 成功，共 {} 字节",
                        midi_bytes.len()
                    );

                    tracing::info!("[DMS导入] 步骤3: 直接加载 MIDI 字节");
                    match lumino_midi_loader::loader::load_parsed_midi_from_bytes(
                        midi_bytes,
                        track_count as u16,
                        0,
                        None,
                    )
                    .await
                    {
                        Ok(parsed_midi) => {
                            tracing::info!(
                                "[DMS导入] 步骤4: DMS 转换的 MIDI 加载成功, 轨道数={}",
                                parsed_midi.info.track_count
                            );
                            tracing::info!("[DMS导入] 步骤5: 发送 MidiParsed 事件");
                            lumino_ui::event::Event::menu_file(
                                lumino_ui::event::menu::file::Event::midi_parsed(
                                    std::sync::Arc::new(parsed_midi),
                                ),
                            );
                            tracing::info!("[DMS导入] 事件发送完成");
                        }
                        Err(e) => {
                            tracing::error!("[DMS导入] 加载转换后的 MIDI 失败: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("[DMS导入] DMS 转换为 MIDI 失败: {}", e);
                    lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                        lumino_ui::event::menu::file::Event::dms_parse_error(format!(
                            "DMS 转换为 MIDI 失败: {e}"
                        )),
                    ));
                }
            }
        });
    }
}
