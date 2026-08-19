//! Runner 文件菜单处理

mod editor_midi;
mod export;
mod helpers;
mod load;
mod material;
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
            Exit => {
                // 保存期间禁止关闭软件：退出请求转为待关闭，保存完成后自动退出
                if self.is_saving() || self.is_cloud_saving() {
                    tracing::info!("保存进行中，退出请求延迟到保存完成");
                    self.window_state.window.close_pending = true;
                } else {
                    event_loop.exit();
                }
            }
            New => self.handle_new_file(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            ImportFromCloud => {
                self.ensure_cloud_ready(crate::runner::cloud::CloudIntent::Import);
            }
            SaveToCloud => {
                self.ensure_cloud_ready(crate::runner::cloud::CloudIntent::Save);
            }
            Save => self.handle_save_file(),
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
            }
            MidiParseError(err) => {
                tracing::error!("MIDI 文件解析失败：{}", err);
                if self.test_state.test_mode_state.is_some() {
                    tracing::error!("测试模式因 MIDI 加载失败而退出");
                    event_loop.exit();
                }
            }
            Close => {
                self.midi_state.current_midi = None;
                self.midi_state.current_midi_source = None;
                self.midi_state.cloud_source = None;
                lumino_extras::palette::unlock_palette();
                self.window_state
                    .window
                    .ui_mut()
                    .dispose_texture_waterfall();
                // 工程级数据必须随工程关闭一起归零：编辑计时/创建时间
                // （clear_editor 内部已重置工程设置对话框状态）
                self.session_tracker.reset();
                self.window_state.window.ui_mut().clear_editor();
                // 恢复主窗口默认标题（工程设置确认时设置的 "{标题} - Lumino"）
                self.window_state.window.window().set_title("Lumino");
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
        self.midi_state.cloud_source = None;
        lumino_extras::palette::unlock_palette();

        // 工程级数据随新工程一起归零：编辑计时/创建时间
        // （clear_editor 内部已重置工程设置对话框状态）
        self.session_tracker.reset();

        // 清空编辑器
        self.window_state.window.ui_mut().clear_editor();

        // 恢复主窗口默认标题（工程设置确认时设置的 "{标题} - Lumino"）
        self.window_state.window.window().set_title("Lumino");

        tracing::info!("已创建新工程");
    }

    /// 保存完成：记录路径 + 底边栏提示（3 秒）+ 云端自动回传
    fn handle_save_completed(&mut self, path: std::path::PathBuf) {
        // 记录保存路径（后续 Ctrl+S 直接覆盖保存原文件）
        self.midi_state.current_midi_source = Some(path.clone());
        tracing::info!("工程保存完成，路径已记录：{:?}", path);

        // 底边栏显示"文件已经保存"，3 秒后自动恢复"就绪"
        let language = self.window_state.window.ui().settings().display.language;
        let saved_msg = lumino_extras::i18n::main_translations(language)
            .status_file_saved
            .to_string();
        self.window_state
            .window
            .ui_mut()
            .set_status_message(Some(saved_msg));
        self.spawn_status_hint_timeout();

        // 从云端打开的文件：自动上传回云端原路径
        // （若已有上传进行中，run_cloud_upload_overwrite 会直接拒绝本次回传，
        //   不排队不补传——用户可待上传完成后再次 Ctrl+S）
        if let Some(src) = self.midi_state.cloud_source.clone() {
            tracing::info!("工程来自云端，自动上传回原路径：{}", src.remote_path);
            self.run_cloud_upload_overwrite(src.conn_id, src.remote_path, path);
        }
    }

    /// 保存失败：底边栏提示错误原因（3 秒后自动恢复）
    fn handle_save_failed(&mut self, msg: String) {
        tracing::error!("保存失败：{msg}");
        let language = self.window_state.window.ui().settings().display.language;
        let fail_msg = format!(
            "{}：{msg}",
            lumino_extras::i18n::main_translations(language).status_save_failed
        );
        self.window_state
            .window
            .ui_mut()
            .set_status_message(Some(fail_msg));
        self.spawn_status_hint_timeout();
    }

    /// 3 秒后发送提示超时事件，清除底边栏状态消息
    fn spawn_status_hint_timeout(&self) {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            lumino_ui::event::emit(lumino_ui::event::Event::menu_file(
                lumino_ui::event::menu::file::Event::save_hint_timeout(),
            ));
        });
    }
}

/// 从文件路径读取文件创建时间并格式化为本地时间字符串
fn format_created_at_from_path(path: &std::path::Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    let created = metadata.modified().ok()?;
    let since_epoch = created
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs() as i64;
    let datetime = chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .unwrap_or_else(chrono::Local::now);
    Some(datetime.format("%Y-%m-%d %H:%M:%S").to_string())
}
