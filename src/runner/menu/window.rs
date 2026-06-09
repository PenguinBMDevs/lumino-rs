//! Runner 窗口事件处理

use crate::runner::{RunnerInner, dialog_manager::DialogType};
use lumino_ui::event::window::Event as WindowEvent;

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: WindowEvent) {
        match window_event {
            // 对话框相关事件
            WindowEvent::OpenCustomPrecisionDialog
            | WindowEvent::CloseCustomPrecisionDialog
            | WindowEvent::ApplyCustomPrecision(_, _)
            | WindowEvent::OpenCollaborationDialog
            | WindowEvent::CloseCollaborationDialog
            | WindowEvent::OpenProjectSettingsDialog
            | WindowEvent::CloseProjectSettingsDialog
            | WindowEvent::ApplyProjectSettings { .. }
            | WindowEvent::OpenSpeedChangeDialog
            | WindowEvent::CloseSpeedChangeDialog
            | WindowEvent::ConfirmSpeedChange(_) => {
                self.handle_dialog_events(window_event);
            }

            // 协作功能相关事件
            WindowEvent::CollaborationConnect { .. }
            | WindowEvent::CollaborationCreateRoom { .. }
            | WindowEvent::CollaborationJoinRoom { .. }
            | WindowEvent::CollaborationDisconnect
            | WindowEvent::CollaborationAuthenticated { .. }
            | WindowEvent::CollaborationRoomCreated { .. }
            | WindowEvent::CollaborationRoomJoined { .. }
            | WindowEvent::CollaborationDisconnected
            | WindowEvent::CollaborationMouseUpdate { .. }
            | WindowEvent::CollaborationNoteUpdate { .. }
            | WindowEvent::CollaborationUserLeft { .. } => {
                self.handle_collaboration_events(window_event);
            }

            // 本地音符事件
            WindowEvent::LocalNoteAdded {
                tick,
                key,
                length,
                velocity,
                channel,
                track_index,
            } => {
                self.handle_local_note_added(tick, key, length, velocity, channel, track_index);
            }
            WindowEvent::LocalNoteMoved {
                tick,
                key,
                length,
                tick_offset,
                key_offset,
                track_index,
            } => {
                self.handle_local_note_moved(
                    tick,
                    key,
                    length,
                    tick_offset,
                    key_offset,
                    track_index,
                );
            }

            _ => {
                // 其他窗口事件暂不处理
            }
        }
    }

    fn handle_dialog_events(&mut self, window_event: WindowEvent) {
        match window_event {
            WindowEvent::OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::CustomPrecision);
            }
            WindowEvent::CloseCustomPrecisionDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::CustomPrecision);
                tracing::info!("请求关闭自定义精度对话框");
            }
            WindowEvent::ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            WindowEvent::OpenCollaborationDialog => {
                tracing::info!("请求打开协作对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::Collaboration);
            }
            WindowEvent::CloseCollaborationDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("请求关闭协作对话框");
            }
            WindowEvent::OpenProjectSettingsDialog => {
                tracing::info!("请求打开工程设置对话框");
                // 优先使用已保存的项目标题，回退到文件名
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
            WindowEvent::CloseProjectSettingsDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::ProjectSettings);
                tracing::info!("请求关闭工程设置对话框");
            }
            WindowEvent::ApplyProjectSettings {
                title,
                tempo,
                copyright,
            } => {
                tracing::info!(
                    "应用工程设置: 标题={}, BPM={}, 版权={}",
                    title,
                    tempo,
                    copyright
                );
                // 应用设置到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_project_settings(title, tempo, copyright);
            }
            WindowEvent::OpenSpeedChangeDialog => {
                tracing::info!("请求打开音符变速对话框");
                self.window_state
                    .dialog_manager
                    .open_dialog(DialogType::SpeedChange);
            }
            WindowEvent::CloseSpeedChangeDialog => {
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::SpeedChange);
                tracing::info!("请求关闭音符变速对话框");
            }
            WindowEvent::ConfirmSpeedChange(factor) => {
                tracing::info!("应用音符变速: 倍率={}", factor);
                // 应用变速到主窗口
                let main_ui = self.window_state.window.ui_mut();
                main_ui.apply_speed_change(factor);
            }
            _ => {}
        }
    }

    fn handle_collaboration_events(&mut self, window_event: WindowEvent) {
        match window_event {
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
                let room_name = Some("Lumino 房间".to_string());
                self.handle_collaboration_connect(host, port, username, room_name, invite_code);
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

                self.window_state
                    .window
                    .ui_mut()
                    .root_mut()
                    .state_mut()
                    .collaboration_dialog
                    .connection_status
                    .clear();

                if let Some(target_invite_code) = self.collab_state.pending_invite_code.take() {
                    tracing::info!("使用首屏填写的邀请码直接加入房间: {}", target_invite_code);
                    self.handle_collaboration_join_room(target_invite_code);
                } else {
                    self.window_state
                        .window
                        .ui_mut()
                        .set_collaboration_view_state(
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
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
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
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
                        lumino_ui::CollaborationViewState::InRoom,
                        Some(invite_code),
                        Some(room_name),
                    );
                self.window_state
                    .dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
            }
            WindowEvent::CollaborationDisconnected => {
                tracing::info!("协作: 连接断开事件");
                self.window_state
                    .window
                    .ui_mut()
                    .set_collaboration_view_state(
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
                username,
            } => {
                tracing::debug!(
                    "窗口事件 - 远程鼠标更新：user_id={}, x={}, y={}, color={}, username={}",
                    user_id,
                    x,
                    y,
                    color,
                    username
                );
                self.window_state
                    .window
                    .ui_mut()
                    .update_remote_cursor(user_id, x, y, color, username);
                self.window_state.window.window().request_redraw();
            }
            WindowEvent::CollaborationNoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
                self.window_state.window.window().request_redraw();
            }
            WindowEvent::CollaborationUserLeft { user_id } => {
                self.window_state
                    .window
                    .ui_mut()
                    .remove_remote_cursor(user_id);
                self.window_state.window.window().request_redraw();
            }
            _ => {}
        }
    }
}
