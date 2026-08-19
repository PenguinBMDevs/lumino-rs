use crate::types::{InviteCode, RemoteUser, RoomInfo, UserId};

/// 客户端状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    /// 已断开连接
    Disconnected,
    /// 正在连接
    Connecting,
    /// 已连接
    Connected,
    /// 正在认证
    Authenticating,
    /// 认证完成
    Authenticated,
    /// 已加入房间
    InRoom,
    /// 出现错误
    Error,
}

/// 协作会话信息
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    /// 当前用户 ID
    pub current_user_id: Option<UserId>,
    /// 当前房间邀请码
    pub invite_code: Option<InviteCode>,
    /// 当前所在房间信息
    pub current_room: Option<RoomInfo>,
    /// 远程在线用户映射
    pub remote_users: std::collections::HashMap<UserId, RemoteUser>,
}
