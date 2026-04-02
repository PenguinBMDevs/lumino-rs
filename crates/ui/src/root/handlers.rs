//! Root 消息处理器子模块
//!
//! 包含所有消息处理逻辑，按功能分组

use crate::root::Root;
use crate::state::root_state::{CollaborationViewState, DialogType};
use crate::{message, sidebar, toolbar, window};

impl Root {
    /// 主更新入口
    pub fn update(&mut self, msg: message::Message) {
        match msg {
            message::Message::Core(event) => self.handle_core_event(event),
            message::Message::Window(event) => self.handle_window_event(event),
            message::Message::Sidebar(event) => self.handle_sidebar_event(event),
            message::Message::Toolbar(event) => self.handle_toolbar_event(event),
            message::Message::Progress(progress) => self.progress = progress,
            message::Message::ScrollbarScrolled(x) => self.editor.set_scroll_x(x),
            message::Message::ScrollbarScrolledY(y) => self.editor.set_scroll_y(y),
            message::Message::ZoomXChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_x(zoom, fixed_ratio);
            }
            message::Message::ZoomYChanged { zoom, fixed_ratio } => {
                self.editor.set_zoom_y(zoom, fixed_ratio);
            }
            message::Message::CanvasBoundsChanged { offset, size } => {
                self.editor.set_canvas_offset(offset);
                self.editor
                    .set_canvas_size(iced_core::Point::new(size.width, size.height));
            }
            message::Message::EditorAction(action) => {
                let old_tick = self.editor.playback_position;
                self.editor.handle_action(action);
                let new_tick = self.editor.playback_position;
                if (old_tick - new_tick).abs() > f32::EPSILON {
                    if let Some(manager) = &mut self.playback_manager {
                        manager.seek(new_tick);
                    }
                }

                // 检查音符数据是否变化，如果变化则更新播放管理器
                if self.editor.notes_changed() {
                    self.update_playback_notes();
                    self.editor.clear_notes_changed();
                }
            }
            message::Message::AudioAction(_) => {
                // 音频动作处理（留给外层实现）
            }
            message::Message::MenuStateChanged(is_open) => {
                self.state.is_menu_open = is_open;
            }
            message::Message::Settings(event) => {
                self.settings.update(event);
            }
            message::Message::ToggleSettings => {
                // ToggleSettings 消息已废弃
            }
            message::Message::Null => {}
            // 协作相关消息
            message::Message::OpenCollaborationDialog => {
                self.handle_collaboration_dialog_open();
            }
            message::Message::CloseCollaborationDialog => {
                self.handle_collaboration_dialog_close();
            }
            message::Message::CollaborationConnect {
                host,
                port,
                username,
                invite_code,
            } => {
                self.handle_collaboration_connect(host, port, username, invite_code);
            }
            message::Message::CollaborationCreateRoom { name } => {
                self.handle_collaboration_create_room(name);
            }
            message::Message::CollaborationJoinRoom { invite_code } => {
                self.handle_collaboration_join_room(invite_code);
            }
            message::Message::CollaborationDisconnect => {
                self.handle_collaboration_disconnect();
            }
            message::Message::CollaborationHostChanged(host) => {
                self.state.collaboration_dialog.server_host = host;
            }
            message::Message::CollaborationPortChanged(port) => {
                self.state.collaboration_dialog.server_port = port;
            }
            message::Message::CollaborationUsernameChanged(username) => {
                self.state.collaboration_dialog.username = username;
            }
            message::Message::CollaborationRoomNameChanged(name) => {
                self.state.collaboration_dialog.room_name = name;
            }
            message::Message::CollaborationInviteCodeChanged(code) => {
                self.state.collaboration_dialog.invite_code = code;
            }
            message::Message::CollaborationCopyInviteCode => {
                self.handle_collaboration_copy_invite_code();
            }
            message::Message::CollaborationRemoteMouseMoved {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                tracing::debug!(
                    "收到远程鼠标移动：user_id={}, x={}, y={}, color={}, username={}",
                    user_id,
                    x,
                    y,
                    color,
                    username
                );
                self.editor.update_remote_cursor(
                    user_id,
                    iced_core::Point::new(x, y),
                    color,
                    username,
                );
                tracing::debug!(
                    "更新后 remote_cursors 数量：{}",
                    self.editor.remote_cursors.len()
                );
            }
            message::Message::CollaborationRemoteUserLeft { user_id } => {
                self.editor.remove_remote_cursor(&user_id);
            }
            message::Message::CollaborationRemoteNoteUpdate {
                user_id: _,
                operation,
            } => {
                // 解析操作并应用到编辑器
                if let Ok(op) = serde_json::from_str::<
                    lumino_collaboration::types::NoteBatchOperation,
                >(&operation)
                {
                    self.apply_remote_note_operation(&op);
                } else {
                    tracing::error!("协作: 无法解析远程笔记操作");
                }
            }
            // 自定义精度对话框消息
            message::Message::OpenCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_open();
            }
            message::Message::CloseCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_close();
            }
            message::Message::ConfirmCustomPrecision => {
                self.handle_confirm_custom_precision();
            }
            message::Message::CustomPrecisionNumeratorChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.state.custom_precision_dialog.tuplet_count = value;
                }
            }
            message::Message::CustomPrecisionDenominatorChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.state.custom_precision_dialog.note_value = value;
                }
            }
            message::Message::CustomPrecisionTupletCountChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.state.custom_precision_dialog.tuplet_count = value;
                }
            }
            message::Message::CustomPrecisionTupletTypeChanged(value) => {
                self.state.custom_precision_dialog.tuplet_type = value;
                self.state.custom_precision_dialog.tuplet_count = value.value().to_string();
            }
            message::Message::CustomPrecisionDotTypeChanged(value) => {
                self.state.custom_precision_dialog.dot_type = value;
            }
            message::Message::CustomPrecisionNoteValueChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.state.custom_precision_dialog.note_value = value;
                }
            }
            message::Message::CustomPrecisionDivisorChanged(value) => {
                if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
                    self.state.custom_precision_dialog.divisor = value;
                }
            }
            // 播放控制消息
            message::Message::Play => {
                self.handle_play();
            }
            message::Message::Pause => {
                self.handle_pause();
            }
            message::Message::Stop => {
                self.handle_stop();
            }
            message::Message::PlaybackTick(tick) => {
                // 更新播放指示线位置
                self.editor.playback_position = tick;

                // 更新播放管理器
                if let Some(manager) = &mut self.playback_manager {
                    manager.update();
                }
            }
        }
    }

    /// 处理核心事件
    fn handle_core_event(&mut self, event: lumino_core::event::Event) {
        // 当执行菜单操作时，关闭菜单
        self.set_menu_open(false);
        lumino_core::event::emit(event);
    }

    /// 处理窗口事件
    fn handle_window_event(&mut self, event: window::Event) {
        // 检测主题是否变化，主题变化时需要清除 grid_cache
        let is_theme_change = matches!(event, window::Event::Theme(_));
        self.window.update(event);
        if is_theme_change {
            self.editor.grid_cache.clear();
        }
    }

    /// 处理侧边栏事件
    fn handle_sidebar_event(&mut self, event: sidebar::Event) {
        // 先检查是否是音轨切换（避免所有权问题）
        let track_selected_idx = if let sidebar::Event::TrackSelected(idx) = &event {
            Some(*idx)
        } else {
            None
        };

        self.sidebar.update(event);

        // 侧边栏显示状态变化，直接设置 canvas offset 为 sidebar 宽度
        let sidebar_width = self.sidebar.width() as f32;
        let current_offset = self.editor.canvas_offset;
        self.editor
            .set_canvas_offset(iced_core::Point::new(sidebar_width, current_offset.y));

        // 如果是音轨切换，发送 Core 事件通知 Runner 加载对应音轨的音符
        if let Some(track_idx) = track_selected_idx {
            tracing::debug!("Root: 发射音轨选择事件，音轨 {}", track_idx);
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::File(
                    lumino_core::event::menu::file::Event::TrackSelected(track_idx),
                ),
            ));
        }
    }

    /// 处理工具栏事件
    fn handle_toolbar_event(&mut self, event: toolbar::Event) {
        // 处理播放控制
        match &event {
            toolbar::Event::Play => {
                self.handle_play();
            }
            toolbar::Event::Pause => {
                self.handle_pause();
            }
            toolbar::Event::Stop => {
                self.handle_stop();
            }
            _ => {}
        }

        // 如果工具切换了，同步更新 editor 的工具状态
        if let toolbar::Event::ToolSelected(tool) = &event {
            self.editor.set_tool(*tool);
        }
        // 如果精度设置变更了，同步更新 editor 的 snap_precision
        if let toolbar::Event::PrecisionChanged(precision) = &event {
            let ticks = (*precision).as_ticks(self.editor.state.ppq);
            self.editor.state.snap_precision = ticks;
            self.editor.state.default_note_length = ticks;
            tracing::debug!(
                "Root: 音符精度同步为 {} ticks (PPQ={})",
                ticks,
                self.editor.state.ppq
            );
        }
        // 处理撤销/重做事件
        if matches!(event, toolbar::Event::Undo) {
            tracing::info!("Root: 触发撤销操作");
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::Edit(lumino_core::event::menu::edit::Event::Undo),
            ));
        }
        if matches!(event, toolbar::Event::Redo) {
            tracing::info!("Root: 触发重做操作");
            lumino_core::event::emit(lumino_core::event::Event::Menu(
                lumino_core::event::menu::Event::Edit(lumino_core::event::menu::edit::Event::Redo),
            ));
        }
        // 处理打开协作对话框事件
        if matches!(event, toolbar::Event::OpenCollaborationDialog) {
            tracing::info!("Root: 触发打开协作对话框");
            lumino_core::event::emit(lumino_core::event::Event::Window(
                lumino_core::event::window::Event::OpenCollaborationDialog,
            ));
        }
        self.toolbar.update(event);
    }

    /// 处理播放
    fn handle_play(&mut self) {
        // 如果播放管理器还未初始化，创建它
        if self.playback_manager.is_none() {
            self.init_playback_manager();
        }

        if let Some(manager) = &mut self.playback_manager {
            manager.play();
            tracing::info!("Root: 开始播放");
        }
    }

    /// 处理暂停
    fn handle_pause(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            manager.pause();
            tracing::info!("Root: 暂停播放");
        }
    }

    /// 处理停止
    fn handle_stop(&mut self) {
        if let Some(manager) = &mut self.playback_manager {
            manager.stop();
            self.editor.playback_position = 0.0;
            tracing::info!("Root: 停止播放");
        }
    }

    /// 初始化播放管理器
    fn init_playback_manager(&mut self) {
        let division = self.editor.state.ppq;
        let mut manager = crate::playback::PlaybackManager::new(division);

        // 设置音符
        let notes: Vec<crate::playback::NoteEvent> = self
            .editor
            .notes
            .iter()
            .map(|note| {
                crate::playback::NoteEvent {
                    tick: note.tick,
                    channel: 0, // 默认通道0
                    key: note.key as u8,
                    velocity: 100, // 默认力度
                    length: note.length,
                }
            })
            .collect();
        manager.set_notes(notes);

        // 应用缓存的 tempo 变化
        if let Some(changes) = self.pending_tempo_changes.take() {
            manager.set_tempo_changes(changes);
        }

        // 应用缓存的 MIDI 输出连接
        if let Some(output) = self.pending_midi_output.take() {
            manager.set_midi_output(output);
        }

        self.playback_manager = Some(manager);
        tracing::info!(
            "Root: 播放管理器已初始化 (division={}, notes={})",
            division,
            self.editor.notes.len()
        );
    }
}
