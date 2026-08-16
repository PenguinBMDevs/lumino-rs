use serde::{Deserialize, Serialize};

use super::alias::{InviteCode, RoomId, UserColor, UserId};

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    pub id: UserId,
    pub username: String,
    pub color: UserColor,
    pub is_host: bool,
}

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: RoomId,
    pub invite_code: InviteCode,
    pub name: String,
    pub host_id: UserId,
    pub user_count: usize,
    pub max_users: usize,
}

/// 用户数据（认证响应中的 user 字段）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UserData {
    pub id: String,
    pub username: String,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}
#[derive(Debug, Clone)]
pub struct RemoteUser {
    pub info: UserInfo,
    pub mouse_position: Option<super::view::MousePosition>,
    pub last_active: std::time::Instant,
}
