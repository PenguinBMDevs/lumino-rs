//! Runner 文件菜单处理

mod export;
mod helpers;
mod load;
mod save;

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理文件菜单事件
    pub(super) fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_ui::event::menu::file::Event,
    ) {
        use lumino_ui::event::menu::file::Event::*;

        match file_event {
            Exit => event_loop.exit(),
            New => self.handle_new_file(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            Save => self.handle_save_file(),
            MidiLoaded(info) => {
                tracing::info!("MIDI 文件加载完成：{}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI 文件加载失败：{}", err);
            }
            MidiParsed(parsed) => {
                tracing::info!("MIDI 文件解析完成：{}", parsed.info);

                // 先导入音符到编辑器（新的懒加载模式：只加载当前音轨，其他音轨按需加载）
                self.import_midi_to_editor(&parsed);

                tracing::debug!("MIDI 文档已导入编辑器，MidiDocument 保留供懒加载使用");

                self.log_memory_usage_after_import();

                // 保留 current_midi 使 MidiDocument 存活（编辑器通过 Arc 引用它做懒加载）
                // 不再需要保存一份全量 track_notes，所以总内存从 (events+notes) 降到 (events)
                self.midi_state.current_midi_source =
                    Some(std::path::PathBuf::from(&parsed.info.path));
                self.midi_state.current_midi = Some(parsed);

                if let Some(state) = &mut self.test_state.test_mode_state {
                    state.active = true;
                }
            }
            MidiParseError(err) => {
                tracing::error!("MIDI 文件解析失败：{}", err);
                if self.test_state.test_mode_state.is_some() {
                    tracing::error!("测试模式因 MIDI 加载失败而退出");
                    event_loop.exit();
                }
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成：{}", parsed.info);

                // 导入 DMS 到编辑器
                self.import_dms_to_editor(&parsed);

                self.midi_state.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败：{}", err);
            }
            Close => {
                self.midi_state.current_midi = None;
                self.midi_state.current_midi_source = None;
                self.midi_state.current_dms = None;
                self.window_state.window.ui_mut().clear_editor();
                tracing::info!("工程已关闭");
            }
            ProjectSettings => {
                let saved_title = self.window_state.window.ui().get_project_settings_title();
                let display_title = if saved_title.is_empty() {
                    self.midi_state
                        .current_midi_source
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "无标题".to_string())
                } else {
                    saved_title
                };
                let title = format!("{} - Lumino Midi", display_title);
                self.window_state
                    .dialog_manager
                    .open_project_settings(title);
            }
            Settings => {
                self.window_state
                    .dialog_manager
                    .open_dialog(crate::runner::dialog_manager::DialogType::Settings);
            }
            TrackSelected(track_idx) => {
                // 统一使用 cache-only 模式，只切换音轨索引
                // 播放时从 cache 流式读取，不单独加载音轨到编辑器
                tracing::info!("切换到音轨：{}", track_idx);
                self.window_state
                    .window
                    .ui_mut()
                    .set_current_track(track_idx);
            }
            ExportProjectArchive => {
                self.handle_export_project_archive();
            }
            ExportProjectFolder => {
                self.handle_export_project_folder();
            }
            AudioExport => {
                self.handle_audio_export();
            }
            _ => {
                tracing::debug!("未处理的文件事件：{:?}", file_event);
            }
        }
    }

    /// 创建新文件
    pub(super) fn handle_new_file(&mut self) {
        // 清空当前工程
        self.midi_state.current_midi = None;
        self.midi_state.current_midi_source = None;
        self.midi_state.current_dms = None;

        // 清空编辑器
        self.window_state.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }
}
