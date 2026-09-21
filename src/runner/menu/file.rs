//! Runner 文件菜单处理
//!
//! 大粒度的处理器已拆分到同级子模块，保持本文件「薄而清晰」：
//! - `close`：关闭 / 新建工程与保存确认流程
//! - `export_flow`：导出 MIDI 文件
//! - `save_flow`：保存完成 / 失败提示
//! - `util`：文件创建时间等工具函数

mod close;
mod editor_midi;
mod export;
mod export_flow;
mod helpers;
mod load;
mod material;
mod save;
mod save_flow;
mod util;

use crate::runner::RunnerInner;
use crate::runner::inner::PendingCloseAction;

use self::util::format_created_at_from_path;

impl RunnerInner {
    /// 处理文件菜单事件
    pub(super) fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_ui::event::menu::file::Event,
    ) {
        use lumino_ui::event::menu::file::Event::*;

        match file_event {
            Exit => {
                // 退出前检查未保存更改；有则弹保存确认对话框，无则直接退出
                self.request_close_action(PendingCloseAction::Exit, event_loop);
            }
            New => {
                // 新建前检查未保存更改；有则弹保存确认对话框，无则直接新建
                self.request_close_action(PendingCloseAction::NewProject, event_loop);
            }
            Open => {
                // 打开另一个工程前检查未保存更改；有则弹保存确认对话框
                self.request_close_action(PendingCloseAction::OpenProject, event_loop);
            }
            ImportFiles => self.handle_import_files(),
            ImportFromCloud => {
                self.ensure_cloud_ready(crate::runner::cloud::CloudIntent::Import);
            }
            SaveToCloud => {
                self.ensure_cloud_ready(crate::runner::cloud::CloudIntent::Save);
            }
            Save => {
                self.handle_save_file();
            }
            SaveCompleted(path) => self.handle_save_completed(path),
            SaveFailed(msg) => self.handle_save_failed(msg),
            SaveHintTimeout => {
                // 3 秒提示超时：恢复底边栏默认"就绪"
                self.window_state.window.ui_mut().set_status_message(None);
            }
            MidiLoaded(info) => {
                tracing::info!("MIDI 文件加载完成：{}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI 文件加载失败：{}", err);
            }
            MidiParsed(parsed) => {
                tracing::info!("MIDI 文件解析完成：{}", parsed.info);

                // MIDI 加载后强制使用 Random 调色板并锁定（禁止用户修改）
                lumino_extras::palette::set_current_palette_by_name("Random");
                lumino_extras::palette::lock_palette();

                // 拆出所有权（事件传递路径上 Arc 唯一；极端情况有额外引用时
                // 浅拷贝 ParsedMidi——其 document 为 Arc 浅 clone，代价可忽略）
                let parsed = match std::sync::Arc::try_unwrap(parsed) {
                    Ok(parsed) => parsed,
                    Err(arc) => (*arc).clone(),
                };

                // 在 move 之前保存 info 相关数据（import 后 parsed.document 已移出）
                let source_path = std::path::PathBuf::from(&parsed.info.path);
                // 历史累计创作时间（.lmpj 工程文件跨会话累计）——必须在 parsed move 前提取
                let accumulated_editing_secs = parsed.accumulated_editing_secs;
                // 作者/版权（.lmpj 工程文件携带，常规 MIDI 为空）——必须在 parsed move 前提取，
                // 加载后回填工程设置对话框，修复"保存后重新打开显示空白"
                let project_author = parsed.author.clone();
                let project_copyright = parsed.copyright.clone();

                // 先导入音符到编辑器（新的懒加载模式：只加载当前音轨，其他音轨按需加载）
                self.import_midi_to_editor(parsed);

                tracing::debug!("MIDI 文档已导入编辑器（MidiDocument 已移入 UI 单一权威源）");

                self.log_memory_usage_after_import();

                // 保留 source 路径；document 已通过 import_midi_to_editor 移入 UI
                // （EditorData.document 独占），runner 不再持有文档副本，避免双份数据。
                self.midi_state.current_midi_source = Some(source_path);
                self.midi_state.current_midi = None;

                // 工程级数据随新文件加载一起归零：编辑计时/累计时间
                // （创建时间随后从文件系统重新设置；工程设置对话框的
                //   标题/作者/版权同样重置，防止上一工程的设置残留）
                self.session_tracker.reset();
                // 从 .lmpj 工程文件恢复历史累计创作时间（metadata.stats.working_time_seconds），
                // 常规 MIDI 文件加载时该值为 0——跨会话累计的关键一环。
                self.session_tracker.accumulated_editing_secs = accumulated_editing_secs;
                self.window_state.window.ui_mut().reset_project_settings();

                // 回填 .lmpj 携带的作者/版权到工程设置对话框状态
                // （reset 已清空，此处重新写入，关闭工程后重开面板显示正确值）
                self.window_state
                    .window
                    .ui_mut()
                    .set_project_author_and_copyright(project_author, project_copyright);

                // 设置工程创建时间（从文件系统获取）
                self.session_tracker.created_at = self
                    .midi_state
                    .current_midi_source
                    .as_ref()
                    .and_then(|p| format_created_at_from_path(p));
                if self.session_tracker.created_at.is_some() {
                    tracing::info!(
                        "工程创建时间已设置: {}",
                        self.session_tracker.created_at.as_deref().unwrap_or("")
                    );
                }

                if let Some(state) = &mut self.test_state.test_mode_state {
                    state.active = true;
                }

                // 加载完成后工程处于干净状态（无相对磁盘文件的未保存更改），
                // 标记清零避免误弹保存确认对话框。
                self.window_state.window.ui_mut().mark_project_clean();
            }
            MidiParseError(err) => {
                tracing::error!("MIDI 文件解析失败：{}", err);
                if self.test_state.test_mode_state.is_some() {
                    tracing::error!("测试模式因 MIDI 加载失败而退出");
                    event_loop.exit();
                }
            }
            Close => {
                // 关闭工程前检查未保存更改；有则弹保存确认对话框，无则直接关闭
                self.request_close_action(PendingCloseAction::CloseProject, event_loop);
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

                // 计算真实的创建时间和累计编辑时间
                let created_display = self.session_tracker.created_at.clone().unwrap_or_default();
                let total_editing_time_seconds = self.session_tracker.current_editing_secs();

                // 从编辑器获取当前 BPM 和拍号
                let (tempo, time_signatures, copyright, author) = {
                    let ui = self.window_state.window.ui();
                    let root = ui.root();
                    let tempo = root
                        .editor
                        .editor_state
                        .data
                        .tempo_points
                        .first()
                        .map(|tp| format!("{:.1}", tp.bpm))
                        .unwrap_or_else(|| "120.0".to_string());
                    let time_signatures = root.editor.editor_state.data.time_signatures.clone();
                    // 版权/作者来自对话框状态：同一工程内多次打开保留已填写的值；
                    // 跨工程（关闭/新建/加载新文件）时状态已重置为空，不会残留。
                    let copyright = ui.get_project_copyright();
                    let author = ui.get_project_author();
                    (tempo, time_signatures, copyright, author)
                };

                // 将真实数据设置到 UI 状态中
                self.window_state.window.ui_mut().set_project_settings_data(
                    lumino_ui::root::ProjectSettingsDialogData {
                        title: display_title.clone(),
                        tempo,
                        copyright,
                        author,
                        created_display,
                        total_editing_time_seconds,
                        time_signatures,
                    },
                );

                self.window_state
                    .dialog_manager
                    .open_project_settings(title);
            }
            Settings => {
                // 打开设置面板前刷新云存储快照（云管理页显示最新状态）
                self.refresh_cloud_connections();
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
                    .set_current_track(track_idx, true);
            }
            ExportProjectArchive => {
                self.handle_export_project_archive();
            }
            ExportProjectFolder => {
                self.handle_export_project_folder();
            }
            ExportMaterial => {
                self.handle_export_material();
            }
            ExportMidi => {
                self.handle_export_midi();
            }
            _ => {
                tracing::debug!("未处理的文件事件：{:?}", file_event);
            }
        }
    }
}
