//! 协作功能处理器

use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;
use crate::state::root_state::CollaborationViewState;

/// 协作消息处理器
pub struct CollaborationHandler;

impl CollaborationHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_collaboration_dialog_open(&self, root: &mut Root) {
        root.state.collaboration_dialog.is_open = true;
    }

    fn handle_collaboration_dialog_close(&self, root: &mut Root) {
        root.state.collaboration_dialog.is_open = false;
    }

    fn handle_collaboration_connect(
        &self,
        root: &mut Root,
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    ) {
        tracing::info!(
            "协作连接: host={}, port={}, username={}, invite_code={:?}",
            host,
            port,
            username,
            invite_code
        );

        root.state.collaboration_dialog.view_state = CollaborationViewState::Connecting;

        // 发送核心事件到 Runner 处理实际连接
        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::collaboration_connect(host, port, username, invite_code),
        ));
    }

    fn handle_collaboration_create_room(&self, root: &mut Root, name: String) {
        tracing::info!("创建协作房间: {}", name);

        root.state.collaboration_dialog.view_state = CollaborationViewState::RoomActions;

        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::collaboration_create_room(name),
        ));
    }

    fn handle_collaboration_join_room(&self, root: &mut Root, invite_code: String) {
        tracing::info!("加入协作房间: {}", invite_code);

        root.state.collaboration_dialog.view_state = CollaborationViewState::RoomActions;

        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::collaboration_join_room(invite_code),
        ));
    }

    fn handle_collaboration_disconnect(&self, root: &mut Root) {
        tracing::info!("断开协作连接");

        root.state.collaboration_dialog.reset();

        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::collaboration_disconnect(),
        ));
    }

    fn handle_collaboration_copy_invite_code(&self, root: &mut Root) {
        if !root.state.collaboration_dialog.invite_code.is_empty()
            && let Ok(mut clipboard) = arboard::Clipboard::new()
        {
            if let Err(e) = clipboard.set_text(root.state.collaboration_dialog.invite_code.clone())
            {
                tracing::error!("复制邀请码失败: {}", e);
            } else {
                tracing::info!("邀请码已复制到剪贴板");
            }
        }
    }

    fn handle_remote_mouse_moved(
        &self,
        root: &mut Root,
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    ) {
        tracing::debug!(
            "收到远程鼠标移动: user_id={}, x={}, y={}, color={}, username={}",
            user_id,
            x,
            y,
            color,
            username
        );

        root.editor
            .update_remote_cursor(user_id, x, y, color, username);
    }

    fn handle_remote_note_update(&self, root: &mut Root, operation: String) {
        if let Ok(op) =
            serde_json::from_str::<lumino_collaboration::types::NoteBatchOperation>(&operation)
        {
            root.apply_remote_note_operation(&op);
        } else {
            tracing::error!("协作: 无法解析远程笔记操作");
        }
    }
}

impl Default for CollaborationHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for CollaborationHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::OpenCollaborationDialog => {
                self.handle_collaboration_dialog_open(root);
                None
            }
            Message::CloseCollaborationDialog => {
                self.handle_collaboration_dialog_close(root);
                None
            }
            Message::CollaborationConnect {
                host,
                port,
                username,
                invite_code,
            } => {
                self.handle_collaboration_connect(root, host, port, username, invite_code);
                None
            }
            Message::CollaborationCreateRoom { name } => {
                self.handle_collaboration_create_room(root, name);
                None
            }
            Message::CollaborationJoinRoom { invite_code } => {
                self.handle_collaboration_join_room(root, invite_code);
                None
            }
            Message::CollaborationDisconnect => {
                self.handle_collaboration_disconnect(root);
                None
            }
            Message::CollaborationCopyInviteCode => {
                self.handle_collaboration_copy_invite_code(root);
                None
            }
            Message::CollaborationRemoteMouseMoved {
                user_id,
                x,
                y,
                color,
                username,
            } => {
                self.handle_remote_mouse_moved(root, user_id, x, y, color, username);
                None
            }
            Message::CollaborationRemoteUserLeft { user_id } => {
                root.editor.remove_remote_cursor(&user_id);
                None
            }
            Message::CollaborationRemoteNoteUpdate { operation } => {
                self.handle_remote_note_update(root, operation);
                None
            }
            // 协作状态更新
            Message::CollaborationHostChanged(host) => {
                root.state.collaboration_dialog.server_host = host;
                None
            }
            Message::CollaborationPortChanged(port) => {
                root.state.collaboration_dialog.server_port = port;
                None
            }
            Message::CollaborationUsernameChanged(username) => {
                root.state.collaboration_dialog.username = username;
                None
            }
            Message::CollaborationRoomNameChanged(name) => {
                root.state.collaboration_dialog.room_name = name;
                None
            }
            Message::CollaborationInviteCodeChanged(code) => {
                root.state.collaboration_dialog.invite_code = code;
                None
            }
            other => Some(other),
        }
    }
}
