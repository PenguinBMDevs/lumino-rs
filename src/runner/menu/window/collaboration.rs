//! 协作类窗口事件处理

use crate::runner::RunnerInner;
use lumino_ui::event::window::collaboration::Event;
use lumino_ui::state::root_state::CollaborationViewState;

impl RunnerInner {
    pub(crate) fn handle_collaboration_events(&mut self, window_event: Event) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            Connect {
                host,
                port,
                username,
                password,
                invite_code,
            } => {
                tracing::info!("请求连接协作服务器: {host}:{port}");
                self.handle_collaboration_connect(
                    host,
                    port,
                    username,
                    password,
                    None,
                    invite_code,
                );
            }
            CreateRoom { name } => {
                tracing::info!("请求创建协作房间: {name}");
                self.handle_collaboration_create_room(name);
            }
            JoinRoom { invite_code } => {
                tracing::info!("请求加入协作房间: {invite_code}");
                self.handle_collaboration_join_room(invite_code);
            }
            Disconnect => {
                tracing::info!("请求断开协作连接");
                self.handle_collaboration_disconnect();
            }
            Authenticated {
                user_id,
                invite_code,
            } => {
                tracing::info!("协作认证成功: user={user_id}, invite={invite_code}");
                // 认证成功：保持连接中（即将进入房间）
                self.set_main_collab_view_state(CollaborationViewState::Connecting, None, None);
            }
            RoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!("协作房间创建成功: {room_name}, invite={invite_code}");
                self.set_main_collab_view_state(
                    CollaborationViewState::InRoom,
                    Some(invite_code),
                    Some(room_name),
                );
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "已加入协作房间: {room_name}, invite={invite_code}, 用户数={user_count}"
                );
                self.set_main_collab_view_state(
                    CollaborationViewState::InRoom,
                    Some(invite_code),
                    Some(room_name),
                );
            }
            Disconnected => {
                tracing::info!("协作连接已断开");
                // 回到可连接态，允许重试
                self.set_main_collab_view_state(CollaborationViewState::Connect, None, None);
            }
            ConnectFailed { reason } => {
                tracing::error!("协作连接失败: {reason}");
                // 连接失败：回到可连接态并展示原因，允许重试
                self.set_main_collab_view_state(
                    CollaborationViewState::Connect,
                    None,
                    Some(reason),
                );
            }
            UserLeft { user_id } => {
                // 从主窗口与协作对话框移除远端光标
                self.window_state
                    .window
                    .ui_mut()
                    .remove_remote_cursor(user_id.clone());
                self.window_state
                    .dialog_manager
                    .forward_collaboration_user_left(user_id);
            }
            MouseUpdate {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                tracing::trace!("协作鼠标更新: user={user_id}, ({x:.0},{y:.0})");
                // 更新主窗口远端光标（编辑器画布渲染）
                self.window_state.window.ui_mut().update_remote_cursor(
                    user_id.clone(),
                    x,
                    y,
                    color.clone(),
                    username.clone(),
                );
                // 同步到协作对话框（对话框亦展示远端光标）
                self.window_state
                    .dialog_manager
                    .forward_collaboration_cursor(user_id, x, y, color, username);
            }
            NoteUpdate { user_id, operation } => {
                self.handle_remote_note_update(user_id, operation);
            }
            ProjectUpdate { user_id, update } => {
                self.handle_remote_project_update(user_id, update);
            }
        }
    }
}
