//! Runner 窗口事件处理

use crate::runner::{RunnerInner, dialog_manager::DialogType};

impl RunnerInner {
    /// 处理窗口事件
    pub(super) fn handle_window_event(&mut self, window_event: lumino_core::event::window::Event) {
        use lumino_core::event::window::Event as WindowEvent;

        match window_event {
            WindowEvent::OpenCustomPrecisionDialog => {
                tracing::info!("请求打开自定义精度对话框");
                self.dialog_manager.open_dialog(DialogType::CustomPrecision);
            }
            WindowEvent::CloseCustomPrecisionDialog => {
                self.dialog_manager
                    .mark_dialog_for_close(DialogType::CustomPrecision);
                tracing::info!("请求关闭自定义精度对话框");
            }
            WindowEvent::ApplyCustomPrecision(_, _) => {
                // 应用精度（在对话框结果中处理）
            }
            WindowEvent::OpenCollaborationDialog => {
                tracing::info!("请求打开协作对话框");
                self.dialog_manager.open_dialog(DialogType::Collaboration);
            }
            WindowEvent::CloseCollaborationDialog => {
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
                // 如果有邀请码则加入房间，否则创建房间
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

                // 清除连接状态
                self.window
                    .ui_mut()
                    .root_mut()
                    .state_mut()
                    .collaboration_dialog
                    .connection_status
                    .clear();

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
                // 自动关闭协作对话框
                self.dialog_manager
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
                // 更新 UI 状态为 InRoom
                self.window.ui_mut().set_collaboration_view_state(
                    lumino_ui::CollaborationViewState::InRoom,
                    Some(invite_code),
                    Some(room_name),
                );
                // 自动关闭协作对话框
                self.dialog_manager
                    .mark_dialog_for_close(DialogType::Collaboration);
                tracing::info!("协作: 自动关闭协作对话框");
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
                self.window
                    .ui_mut()
                    .update_remote_cursor(user_id, x, y, color, username);
                self.window.window().request_redraw();
            }
            WindowEvent::CollaborationNoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
            }
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
            WindowEvent::CollaborationUserLeft { user_id } => {
                self.window.ui_mut().remove_remote_cursor(user_id);
                self.window.window().request_redraw();
            }
            _ => {
                // 其他窗口事件暂不处理
            }
        }
    }
}
