use crate::types::{InviteCode, RemoteUser, RoomInfo, UserId};

/// 客户端状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    InRoom,
    Error,
}

/// 协作会话信息
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    pub current_user_id: Option<UserId>,
    pub invite_code: Option<InviteCode>,
    pub current_room: Option<RoomInfo>,
    pub remote_users: std::collections::HashMap<UserId, RemoteUser>,
}
