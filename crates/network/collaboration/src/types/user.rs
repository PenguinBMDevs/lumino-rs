use serde::{Deserialize, Serialize};

use super::alias::{InviteCode, RoomId, UserColor, UserId};

/// 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    /// 用户 ID
    pub id: UserId,
    /// 用户名
    pub username: String,
    /// 用户头衔颜色
    pub color: UserColor,
    /// 是否为房主
    pub is_host: bool,
}

/// 房间信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    /// 房间 ID
    pub id: RoomId,
    /// 房间邀请码
    pub invite_code: InviteCode,
    /// 房间名称
    pub name: String,
    /// 房主用户 ID
    pub host_id: UserId,
    /// 当前用户数量
    pub user_count: usize,
    /// 最大用户数量
    pub max_users: usize,
}

/// 用户数据（认证响应中的 user 字段）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UserData {
    /// 用户 ID
    pub id: String,
    /// 用户名
    pub username: String,
    /// 附加字段（透传的其它用户数据）
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// 远程用户（在线协作用户）
#[derive(Debug, Clone)]
pub struct RemoteUser {
    /// 用户基本信息
    pub info: UserInfo,
    /// 最近一次鼠标位置
    pub mouse_position: Option<super::view::MousePosition>,
    /// 最近活跃时间
    pub last_active: std::time::Instant,
}
