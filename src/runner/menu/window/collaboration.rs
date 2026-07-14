//! 协作类窗口事件处理

use crate::runner::RunnerInner;
use lumino_ui::event::window::collaboration::Event;

impl RunnerInner {
    pub(crate) fn handle_collaboration_events(&mut self, window_event: Event) {
        use lumino_ui::event::window::collaboration::Event::*;
        match window_event {
            Connect {
                host,
                port,
                username,
                invite_code,
            } => {
                tracing::info!("请求连接协作服务器: {host}:{port}");
                self.handle_collaboration_connect(host, port, username, None, invite_code);
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
            }
            RoomCreated {
                room_name,
                invite_code,
            } => {
                tracing::info!("协作房间创建成功: {room_name}, invite={invite_code}");
            }
            RoomJoined {
                room_name,
                invite_code,
                user_count,
            } => {
                tracing::info!(
                    "已加入协作房间: {room_name}, invite={invite_code}, 用户数={user_count}"
                );
            }
            Disconnected => {
                tracing::info!("协作连接已断开");
            }
            UserLeft { user_id } => {
                tracing::info!("协作用户离开: {user_id}");
            }
            MouseUpdate { user_id, x, y, .. } => {
                tracing::debug!("协作鼠标更新: user={user_id}, ({x:.0},{y:.0})");
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
