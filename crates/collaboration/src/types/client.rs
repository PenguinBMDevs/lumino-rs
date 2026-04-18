use std::collections::HashMap;

use super::alias::{InviteCode, UserId};
use super::user::{RemoteUser, RoomInfo};

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

/// 协作会话状态
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    pub current_room: Option<RoomInfo>,
    pub remote_users: HashMap<UserId, RemoteUser>,
    pub current_user_id: Option<UserId>,
    pub invite_code: Option<InviteCode>,
}

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub server_host: String,
    pub server_port: u16,
    pub username: String,
    pub auto_reconnect: bool,
    pub max_reconnect_attempts: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_millis() as u32;

        Self {
            server_host: "localhost".to_string(),
            server_port: 3000,
            username: format!("用户{}", seed % 10000),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        }
    }
}
