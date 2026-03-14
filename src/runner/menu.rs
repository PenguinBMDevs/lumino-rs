use std::sync::Arc;

use lumino_core::ParsedMidi;
use lumino_core::event;
use lumino_core::midi::constants::DEFAULT_PPQN;
use lumino_ui::TrackNotes;

use super::RunnerInner;
use super::async_helper::run_async_task;

/// MIDI 导出默认力度
const DEFAULT_VELOCITY: u8 = 100;

/// MIDI 格式类型 1
const MIDI_FORMAT_TYPE: u16 = 1;

impl RunnerInner {
    pub(super) fn process_core_events(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let events = lumino_core::event::take_events();
        for event in events {
            self.handle_core_event(event_loop, event);
        }
    }

    fn handle_core_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        event: lumino_core::event::Event,
    ) {
        use lumino_core::event::Event;

        match event {
            Event::Menu(menu_event) => {
                self.handle_menu_event(event_loop, menu_event);
            }
            Event::Window(window_event) => {
                // 如果是鼠标移动，实时同步协作状态
                if matches!(window_event, lumino_core::event::window::Event::Drag) {
                    self.sync_collaboration_state();
                }
                self.handle_window_event(window_event);
            }
        }
    }

    /// 同步协作状态（发送鼠标位置等）
    fn sync_collaboration_state(&mut self) {
        if let Some(client) = &self.collaboration_client {
            let ui = self.window.ui();
            let editor = ui.root().editor_ref();

            if let Some(pos) = editor.cursor_position {
                // 转换为 Canvas 相对坐标并考虑滚动
                let local_pos = iced_core::Point::new(
                    pos.x - editor.canvas_offset.x,
                    pos.y - editor.canvas_offset.y,
                );

                if editor.is_inside_canvas(local_pos) {
                    let client = client.clone();
                    let scroll_x = editor.state.scroll_x;
                    let scroll_y = editor.state.scroll_y;
                    let zoom_x = editor.state.zoom_x;
                    let zoom_y = editor.state.zoom_y;

                    let pos = lumino_collaboration::types::MousePosition {
                        x: local_pos.x,
                        y: local_pos.y,
                        view_state: Some(lumino_collaboration::types::ViewState {
                            scroll_x,
                            scroll_y,
                            zoom_x,
                            zoom_y,
                            ..Default::default()
                        }),
                    };

                    tokio::spawn(async move {
                        let c = client.lock().await;
                        let _ = c.send_mouse_position(pos);
                    });
                }
            }
        }
    }

    fn handle_window_event(&mut self, window_event: lumino_core::event::window::Event) {
        use super::dialog_manager::DialogType;
        use lumino_core::event::window::Event as WindowEvent;

        match window_event {
            WindowEvent::OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                // 打开自定义精度对话框
                self.dialog_manager.open_dialog(DialogType::CustomPrecision);
            }
            WindowEvent::CloseCustomPrecisionDialog => {
                // 关闭自定义精度对话框
                self.dialog_manager
                    .mark_dialog_for_close(DialogType::CustomPrecision);
                tracing::info!("请求关闭自定义精度对话框");
            }
            WindowEvent::ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            WindowEvent::OpenCollaborationDialog => {
                tracing::info!("请求打开协作对话框");
                // 打开协作对话框
                self.dialog_manager.open_dialog(DialogType::Collaboration);
            }
            WindowEvent::CloseCollaborationDialog => {
                // 关闭协作对话框
                self.dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("请求关闭协作对话框");
            }
            WindowEvent::CollaborationConnect {
                host,
                port,
                username,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 连接到 {}:{}, 用户名: {}, 邀请码: {:?}",
                    host,
                    port,
                    username,
                    invite_code
                );
                self.pending_invite_code = invite_code;
                self.handle_collaboration_connect(host, port, username);
            }
            WindowEvent::CollaborationCreateRoom { name } => {
                self.handle_collaboration_create_room(name);
            }
            WindowEvent::CollaborationJoinRoom { invite_code } => {
                self.handle_collaboration_join_room(invite_code);
            }
            WindowEvent::CollaborationDisconnect => {
                self.handle_collaboration_disconnect();
            }
            WindowEvent::CollaborationAuthenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 认证成功事件 - 用户ID: {}, 目前默认邀请码: {}",
                    user_id,
                    invite_code
                );

                if let Some(target_invite_code) = self.pending_invite_code.take() {
                    tracing::info!("使用首屏填写的邀请码直接加入房间: {}", target_invite_code);
                    self.handle_collaboration_join_room(target_invite_code);
                } else {
                    // 更新 UI 状态为 RoomActions
                    self.window.ui_mut().set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::RoomActions,
                        Some(invite_code),
                        None,
                    );
                }
            }
            WindowEvent::CollaborationRoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!(
                    "协作: 房间创建成功 - 房间名: {}, 邀请码: {}",
                    room_name,
                    invite_code
                );
                // 更新 UI 状态为 InRoom
                self.window.ui_mut().set_collaboration_view_state(
                    lumino_ui::CollaborationViewState::InRoom,
                    Some(invite_code),
                    Some(room_name),
                );
            }
            WindowEvent::CollaborationRoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "协作: 加入房间成功 - 房间名: {}, 邀请码: {}, 用户数: {}",
                    room_name,
                    invite_code,
                    user_count
                );
                // 更新 UI 状态为 InRoom
                self.window.ui_mut().set_collaboration_view_state(
                    lumino_ui::CollaborationViewState::InRoom,
                    Some(invite_code),
                    Some(room_name),
                );
            }
            WindowEvent::CollaborationDisconnected => {
                tracing::info!("协作: 连接断开事件");
                // 重置 UI 状态
                self.window.ui_mut().set_collaboration_view_state(
                    lumino_ui::CollaborationViewState::Connect,
                    None,
                    None,
                );
            }
            WindowEvent::CollaborationMouseUpdate {
                user_id,
                x,
                y,
                color,
            } => {
                self.window
                    .ui_mut()
                    .update_remote_cursor(user_id, x, y, color);
            }
            WindowEvent::CollaborationNoteUpdate { user_id, operation } => {
                self.window.ui_mut().update_remote_note(user_id, operation);
            }
            _ => {
                // 其他窗口事件暂不处理
            }
        }
    }

    fn handle_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        menu_event: lumino_core::event::menu::Event,
    ) {
        use lumino_core::event::menu::Event::*;

        match menu_event {
            File(file_event) => {
                self.handle_file_menu_event(event_loop, file_event);
            }
            Edit(_) => {
                // 处理编辑事件（占位）
            }
            View(view_event) => {
                self.handle_view_menu_event(view_event);
            }
            Help(_) => {
                // 处理帮助事件（占位）
            }
        }
    }

    fn handle_file_menu_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        file_event: lumino_core::event::menu::file::Event,
    ) {
        use lumino_core::event::menu::file::Event::*;

        match file_event {
            Exit => event_loop.exit(),
            New => self.handle_new_file(),
            Open => self.handle_open_file(),
            ImportFiles => self.handle_import_files(),
            Save => self.handle_save_file(),
            MidiLoaded(info) => {
                tracing::info!("MIDI文件加载完成: {}", info);
            }
            MidiLoadError(err) => {
                tracing::error!("MIDI文件加载失败: {}", err);
            }
            MidiParsed(mut parsed) => {
                tracing::info!("MIDI文件解析完成: {}", parsed.info);

                // 先导入音符到编辑器
                self.import_midi_to_editor(&parsed);

                // 导入完成后释放原始数据
                let _ = parsed.take_midi_data();
                tracing::debug!("MIDI原始数据已释放，仅保留元数据");

                self.current_midi = Some(parsed);
            }
            MidiParseError(err) => {
                tracing::error!("MIDI文件解析失败: {}", err);
            }
            DmsParsed(parsed) => {
                tracing::info!("DMS 文件解析完成: {}", parsed.info);
                self.current_dms = Some(parsed);
            }
            DmsParseError(err) => {
                tracing::error!("DMS 文件解析失败: {}", err);
            }
            Close => {
                self.current_midi = None;
                self.current_dms = None;
                self.window.ui_mut().clear_editor();
                tracing::info!("工程已关闭");
            }
            TrackSelected(track_idx) => {
                tracing::info!("切换到音轨: {}", track_idx);
                if let Some(memory_manager_arc) = self
                    .current_midi
                    .as_ref()
                    .and_then(|p| p.memory_manager.clone())
                {
                    tracing::debug!("TrackSelected: memory_manager_arc found");
                    if let Ok(mut memory_manager) = memory_manager_arc.lock() {
                        tracing::debug!("TrackSelected: lock acquired");
                        self.midi_handler.load_track_to_editor(
                            &mut memory_manager,
                            track_idx,
                            self.window.ui_mut(),
                        );
                    } else {
                        tracing::warn!("TrackSelected: failed to lock memory_manager");
                    }
                } else {
                    // 没有加载 MIDI 文件，让编辑器处理音轨切换
                    tracing::debug!(
                        "TrackSelected: no MIDI loaded, letting editor handle track switch"
                    );
                    self.window.ui_mut().set_current_track(track_idx);
                }
            }
            _ => {
                tracing::debug!("未处理的文件事件: {:?}", file_event);
            }
        }
    }

    fn handle_new_file(&mut self) {
        // 清空当前工程
        self.current_midi = None;
        self.current_dms = None;

        // 清空编辑器
        self.window.ui_mut().clear_editor();

        tracing::info!("已创建新工程");
    }

    fn handle_open_file(&mut self) {
        // 使用 FileHandler 打开文件对话框
        let Some(path) = self.file_handler.handle_open_file() else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if extension == "dms" {
            self.load_dms_file(path);
        } else {
            self.load_midi_file(path);
        }
    }

    fn load_dms_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 DMS 文件: {:?}", path);
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_dms(path),
                |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                |e| event!(Menu.File.DmsParseError(e)),
            )
            .await;
        });
    }

    fn load_midi_file(&self, path: std::path::PathBuf) {
        tracing::info!("开始后台加载 MIDI 文件: {:?}", path);
        tokio::spawn(async move {
            run_async_task(
                lumino_core::midi::loader::load_parsed_midi(path),
                |parsed| event!(Menu.File.MidiParsed(parsed)),
                |e| event!(Menu.File.MidiParseError(e)),
            )
            .await;
        });
    }

    fn handle_import_files(&self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("音乐文件", &["mid", "midi", "lmpj", "dms"])
            .add_filter("MIDI 文件", &["mid", "midi"])
            .add_filter("Lumino 项目", &["lmpj"])
            .add_filter("Domino 项目", &["dms"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        else {
            return;
        };

        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        tracing::info!("开始后台导入文件: {:?}", path);
        tokio::spawn(async move {
            match extension.as_str() {
                "dms" => {
                    run_async_task(
                        lumino_core::midi::loader::load_dms(path),
                        |parsed| event!(Menu.File.DmsParsed(Arc::new(parsed))),
                        |e| event!(Menu.File.DmsParseError(e)),
                    )
                    .await;
                }
                _ => {
                    // LMPJ 和 MIDI 都使用 MIDI 加载器
                    run_async_task(
                        lumino_core::midi::loader::load_parsed_midi(path),
                        |parsed| event!(Menu.File.MidiParsed(parsed)),
                        |e| event!(Menu.File.MidiParseError(e)),
                    )
                    .await;
                }
            }
        });
    }

    fn handle_save_file(&mut self) {
        // 检查是否加载了MIDI文件
        if let Some(parsed_midi) = &self.current_midi {
            let file_stem = std::path::Path::new(&parsed_midi.info.path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let Some(save_path) = rfd::FileDialog::new()
                .add_filter("Lumino MIDI Project", &["lmpj"])
                .add_filter("MIDI 文件 (.mid)", &["mid"])
                .add_filter("MIDI 文件 (.midi)", &["midi"])
                .add_filter("Domino 项目", &["dms"])
                .set_file_name(format!("{file_stem}.lmpj"))
                .save_file()
            else {
                return;
            };

            let extension = save_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            let file_service = self.file_service.clone();
            let parsed_midi = parsed_midi.clone();

            match extension.as_str() {
                "lmpj" => {
                    tokio::spawn(async move {
                        let _ = file_service.save_as_lmpj(&parsed_midi, save_path).await;
                    });
                }
                "mid" | "midi" => {
                    let source_path = parsed_midi.info.path.clone();
                    tokio::spawn(async move {
                        let _ = file_service.save_as_midi(source_path, save_path).await;
                    });
                }
                "dms" => {
                    let source_path = parsed_midi.info.path.clone();
                    tokio::spawn(async move {
                        let _ = file_service.save_as_dms(source_path, save_path).await;
                    });
                }
                _ => {
                    tracing::warn!("不支持的保存格式: {}", extension);
                }
            }
            return;
        }

        // 检查是否加载了DMS文件
        if let Some(parsed_dms) = &self.current_dms {
            let file_stem = std::path::Path::new(&parsed_dms.info.path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let Some(save_path) = rfd::FileDialog::new()
                .add_filter("Domino 项目", &["dms"])
                .add_filter("MIDI 文件", &["mid", "midi"])
                .set_file_name(format!("{file_stem}.dms"))
                .save_file()
            else {
                return;
            };

            let extension = save_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            let file_service = self.file_service.clone();
            let parsed_dms = parsed_dms.clone();

            match extension.as_str() {
                "dms" => {
                    let source_path = parsed_dms.info.path.clone();
                    tokio::spawn(async move {
                        let _ = file_service.copy_dms_file(source_path, save_path).await;
                    });
                }
                "mid" | "midi" => {
                    let source_path = parsed_dms.info.path.clone();
                    tokio::spawn(async move {
                        let _ = file_service
                            .export_dms_to_midi(source_path, save_path)
                            .await;
                    });
                }
                _ => {
                    tracing::warn!("不支持的保存格式: {}", extension);
                }
            }
        }
    }

    /// 保存编辑器数据为 LMPJ 文件
    fn save_editor_as_lmpj(&self, editor_notes: Vec<TrackNotes>, save_path: std::path::PathBuf) {
        // 创建 ParsedMidi 数据结构
        let parsed_midi = self.build_parsed_midi_from_editor(&editor_notes, &save_path);

        let save_path_log = save_path.clone();
        tokio::spawn(async move {
            lumino_core::midi::loader::send_progress_message("准备保存 LMPJ 文件", 0.0);
            lumino_core::midi::loader::send_progress_message("正在保存 LMPJ 文件", 0.3);
            match lumino_export::save(&parsed_midi, save_path.clone()).await {
                Ok(()) => {
                    lumino_core::midi::loader::send_progress_message("LMPJ 保存成功", 1.0);
                    tracing::info!("新建工程保存成功: {:?}", save_path_log);
                }
                Err(e) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("保存失败: {e}"),
                        1.0,
                    );
                    tracing::error!("新建工程保存失败: {}", e);
                }
            }
        });
    }

    /// 保存编辑器数据为 MIDI 文件
    fn save_editor_as_midi(&self, editor_notes: Vec<TrackNotes>, save_path: std::path::PathBuf) {
        // 构建 MIDI 导出数据
        let midi_data = self.build_midi_export_data(&editor_notes);
        let save_path_log = save_path.clone();

        tokio::spawn(async move {
            lumino_core::midi::loader::send_progress_message("准备导出 MIDI 文件", 0.0);
            lumino_core::midi::loader::send_progress_message("正在导出 MIDI 文件", 0.5);

            match tokio::task::spawn_blocking(move || {
                lumino_export::export_midi_to_bytes(&midi_data)
            })
            .await
            {
                Ok(Ok(bytes)) => {
                    lumino_core::midi::loader::send_progress_message("正在写入文件", 0.8);
                    match std::fs::write(&save_path_log, bytes) {
                        Ok(()) => {
                            lumino_core::midi::loader::send_progress_message("MIDI 导出成功", 1.0);
                            tracing::info!("新建工程导出为 MIDI 成功: {:?}", save_path_log);
                        }
                        Err(e) => {
                            lumino_core::midi::loader::send_progress_message(
                                &format!("写入文件失败: {e}"),
                                1.0,
                            );
                            tracing::error!("MIDI 导出失败: {}", e);
                        }
                    }
                }
                Ok(Err(e)) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("导出失败: {e}"),
                        1.0,
                    );
                    tracing::error!("MIDI 导出失败: {}", e);
                }
                Err(e) => {
                    lumino_core::midi::loader::send_progress_message(
                        &format!("导出失败: {e}"),
                        1.0,
                    );
                    tracing::error!("MIDI 导出失败: {}", e);
                }
            }
        });
    }

    /// 从编辑器音符构建 ParsedMidi
    fn build_parsed_midi_from_editor(
        &self,
        editor_notes: &[TrackNotes],
        save_path: &std::path::Path,
    ) -> lumino_core::ParsedMidi {
        use lumino_core::midi::info::MidiInfo;

        // 计算总音符数和最大 tick
        let total_notes: u64 = editor_notes
            .iter()
            .map(|(_, notes)| notes.len() as u64)
            .sum();
        let max_tick = editor_notes
            .iter()
            .flat_map(|(_, notes)| notes.iter())
            .map(|(tick, _, length)| tick + length)
            .fold(0.0f32, f32::max) as u32;

        // 生成 MIDI 字节流用于保存
        let midi_export_data = self.build_midi_export_data(editor_notes);
        let midi_bytes = match lumino_export::export_midi_to_bytes(&midi_export_data) {
            Ok(bytes) => {
                tracing::info!("生成 MIDI 字节流成功: {} 字节", bytes.len());
                if !bytes.is_empty() {
                    Some(bytes)
                } else {
                    tracing::warn!("生成的 MIDI 字节流为空");
                    None
                }
            }
            Err(e) => {
                tracing::error!("生成 MIDI 字节流失败: {}", e);
                None
            }
        };

        tracing::info!(
            "构建 ParsedMidi: 音轨数={}, 总音符数={}, midi_data={}",
            editor_notes.len(),
            total_notes,
            if midi_bytes.is_some() { "有" } else { "无" }
        );

        lumino_core::ParsedMidi {
            info: MidiInfo {
                path: save_path.to_path_buf(),
                track_count: editor_notes.len() as u16,
                total_notes,
                duration_ticks: max_tick,
                division: DEFAULT_PPQN,
                parse_progress: None,
            },
            midi_data: midi_bytes,
            memory_manager: None,
        }
    }

    /// 从编辑器音符构建 MIDI 导出数据
    fn build_midi_export_data(
        &self,
        editor_notes: &[TrackNotes],
    ) -> lumino_export::midi::MidiExportData {
        use lumino_export::midi::{
            MidiExportData, MidiExportOptions, MidiNoteEvent, MidiTrackData,
        };

        let mut tracks = Vec::new();

        for (track_idx, notes) in editor_notes {
            let track_notes: Vec<MidiNoteEvent> = notes
                .iter()
                .map(|(tick, key, length)| MidiNoteEvent {
                    tick: *tick as u32,
                    channel: 0,
                    key: *key,
                    velocity: DEFAULT_VELOCITY,
                    duration: *length as u32,
                })
                .collect();

            let track_data = MidiTrackData {
                notes: track_notes,
                tempos: vec![],
                program_changes: vec![],
                control_changes: vec![],
                time_signatures: vec![],
                key_signatures: vec![],
                name: Some(format!("Track {}", track_idx + 1)),
            };

            tracks.push(track_data);
        }

        MidiExportData {
            options: MidiExportOptions {
                format: MIDI_FORMAT_TYPE,
                ppqn: DEFAULT_PPQN,
            },
            tracks,
        }
    }

    fn handle_view_menu_event(&mut self, view_event: lumino_core::event::menu::view::Event) {
        use lumino_core::event::menu::view::Event::*;

        match view_event {
            Theme(theme) => {
                self.window.ui_mut().update_theme(theme.clone());
                self.storage.config.patch(|state| {
                    state.ui.theme = theme;
                });
            }
        }
    }

    /// 将 MIDI 数据导入到编辑器
    fn import_midi_to_editor(&mut self, parsed: &ParsedMidi) {
        self.midi_handler
            .import_midi_to_editor(self.window.ui_mut(), parsed);
    }

    /// 处理协作连接
    fn handle_collaboration_connect(&mut self, host: String, port: u16, username: String) {
        use super::CollaborationStatus;

        // 更新状态为连接中
        self.collaboration_status = CollaborationStatus::Connecting;

        // 使用协作服务连接
        let service = self.collaboration_service.clone();
        tokio::spawn(async move {
            if let Err(e) = service.connect(host, port, username).await {
                tracing::error!("协作连接失败: {}", e);
            }
        });
    }

    /// 处理创建房间
    fn handle_collaboration_create_room(&self, name: String) {
        tracing::info!("协作: 请求创建房间 - {}", name);
        let handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.create_room(name).await {
                tracing::error!("协作: 创建房间失败: {}", e);
            }
        });
    }

    /// 处理加入房间
    fn handle_collaboration_join_room(&self, invite_code: String) {
        tracing::info!("协作: 请求加入房间 - {}", invite_code);
        let handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.join_room(invite_code).await {
                tracing::error!("协作: 加入房间失败: {}", e);
            }
        });
    }

    /// 处理断开连接
    fn handle_collaboration_disconnect(&mut self) {
        tracing::info!("协作: 请求断开连接");
        let mut handler = self.collaboration_handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.disconnect().await {
                tracing::error!("协作: 断开连接失败: {}", e);
            }
        });
    }
}
