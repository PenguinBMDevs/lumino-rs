//! 协作客户端消息定义

/// 客户端到服务器的消息
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ClientMessage {
    /// 认证请求
    Auth {
        /// 用户名
        username: String,
        /// 密码（与注册/登录账户一致，用于 WebSocket 握手鉴权）
        password: String,
    },
    /// 创建房间请求
    CreateRoom {
        /// 房间名称
        name: String,
    },
    /// 加入房间请求
    JoinRoom {
        /// 邀请码
        #[serde(rename = "inviteCode")]
        invite_code: String,
    },
    /// 离开房间请求
    LeaveRoom,
    /// 鼠标移动通知
    MouseMove {
        /// 鼠标坐标
        position: crate::types::MousePosition,
    },
    /// 音符批量操作
    NoteBatch {
        /// 音符批量操作内容
        notes: crate::types::NoteBatchOperation,
    },
    /// MIDI 事件
    MidiEvent {
        /// MIDI 事件内容
        event: crate::types::MidiEvent,
    },
    /// MIDI 事件批量操作
    MidiEventBatch {
        /// MIDI 事件列表
        events: Vec<crate::types::MidiEvent>,
    },
    /// 项目状态更新
    ProjectUpdate {
        /// 项目更新内容
        update: crate::types::ProjectUpdate,
    },
    /// 请求全量同步
    RequestSync,
    /// 心跳 Ping
    Ping {
        /// 发送时间戳
        timestamp: u64,
    },
}

/// 服务器到客户端的消息
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ServerMessage {
    /// 认证成功响应（含完整会话信息）
    Authenticated {
        /// 当前用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 房间信息
        room: AuthRoomInfo,
        /// 当前用户数据
        user: crate::types::UserData,
        /// 房间内用户列表
        users: Vec<crate::types::UserInfo>,
    },
    /// 认证成功响应（仅含用户与房间标识）
    AuthSuccess {
        /// 当前用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 房间邀请码
        #[serde(rename = "inviteCode")]
        invite_code: crate::types::InviteCode,
    },
    /// 认证失败响应
    AuthError {
        /// 错误描述
        error: String,
    },
    /// 房间创建成功响应
    RoomCreated {
        /// 创建的房间信息
        room: crate::types::RoomInfo,
    },
    /// 加入房间成功响应
    RoomJoined {
        /// 加入的房间信息
        room: crate::types::RoomInfo,
        /// 房间内用户列表
        users: Vec<crate::types::UserInfo>,
        /// 项目状态快照
        #[serde(rename = "projectState")]
        project_state: crate::types::ProjectState,
    },
    /// 房间错误响应
    RoomError {
        /// 错误描述
        error: String,
    },
    /// 用户加入通知
    UserJoined {
        /// 新加入的用户信息
        user: crate::types::UserInfo,
    },
    /// 用户离开通知
    UserLeft {
        /// 离开用户的 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
    },
    /// 鼠标位置更新
    MouseUpdate {
        /// 移动鼠标的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 用户名
        username: String,
        /// 鼠标坐标
        position: crate::types::MousePosition,
        /// 用户头衔颜色
        color: String,
    },
    /// 音符批量操作更新
    NoteBatchUpdate {
        /// 发起操作的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 音符批量操作内容
        operation: crate::types::NoteBatchOperation,
    },
    /// MIDI 事件更新
    MidiEventUpdate {
        /// 发起事件的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// MIDI 事件内容
        event: crate::types::MidiEvent,
    },
    /// MIDI 事件批量更新
    MidiEventBatchUpdate {
        /// 发起事件的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// MIDI 事件列表
        events: Vec<crate::types::MidiEvent>,
    },
    /// 项目状态更新
    ProjectStateUpdate {
        /// 发起更新的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 项目更新内容
        update: crate::types::ProjectUpdate,
    },
    /// 全量同步响应
    FullSync {
        /// 项目状态快照
        #[serde(rename = "projectState")]
        project_state: crate::types::ProjectState,
        /// 全量用户列表
        users: Vec<crate::types::UserInfo>,
    },
    /// Ping 响应
    Pong {
        /// 客户端发送时的时间戳
        timestamp: u64,
        /// 服务器时间戳
        #[serde(rename = "serverTime")]
        server_time: u64,
    },
    /// 服务器端错误
    Error {
        /// 错误描述
        error: String,
    },
}

impl ServerMessage {
    /// 返回协议中的消息类型标签（与 `#[serde(tag = "type")]` 的 camelCase 一致）
    pub fn type_name(&self) -> &'static str {
        match self {
            ServerMessage::Authenticated { .. } => "authenticated",
            ServerMessage::AuthSuccess { .. } => "authSuccess",
            ServerMessage::AuthError { .. } => "authError",
            ServerMessage::RoomCreated { .. } => "roomCreated",
            ServerMessage::RoomJoined { .. } => "roomJoined",
            ServerMessage::RoomError { .. } => "roomError",
            ServerMessage::UserJoined { .. } => "userJoined",
            ServerMessage::UserLeft { .. } => "userLeft",
            ServerMessage::MouseUpdate { .. } => "mouseUpdate",
            ServerMessage::NoteBatchUpdate { .. } => "noteBatchUpdate",
            ServerMessage::MidiEventUpdate { .. } => "midiEventUpdate",
            ServerMessage::MidiEventBatchUpdate { .. } => "midiEventBatchUpdate",
            ServerMessage::ProjectStateUpdate { .. } => "projectStateUpdate",
            ServerMessage::FullSync { .. } => "fullSync",
            ServerMessage::Pong { .. } => "pong",
            ServerMessage::Error { .. } => "error",
        }
    }
}

/// 认证响应中的房间信息
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthRoomInfo {
    /// 房间 ID
    pub id: String,
    /// 房间邀请码
    #[serde(rename = "inviteCode")]
    pub invite_code: crate::types::InviteCode,
    /// 房间名称
    pub name: String,
    /// 房主用户 ID
    #[serde(rename = "hostId")]
    pub host_id: crate::types::UserId,
    /// 当前用户数量
    #[serde(rename = "userCount")]
    pub user_count: u32,
    /// 最大用户数量
    #[serde(rename = "maxUsers")]
    pub max_users: u32,
}

#[cfg(test)]
mod tests {
    use super::ServerMessage;

    /// type_name 必须与 serde 协议标签一致（`#[serde(tag = "type")]` camelCase），
    /// 否则兜底日志会输出与线上协议不一致的标签
    #[test]
    fn test_server_message_type_name_matches_serde_tag() {
        let cases = [
            (r#"{"type":"pong","timestamp":1,"serverTime":2}"#, "pong"),
            (r#"{"type":"userLeft","userId":"u1"}"#, "userLeft"),
            (r#"{"type":"roomError","error":"boom"}"#, "roomError"),
            (r#"{"type":"error","error":"boom"}"#, "error"),
        ];
        for (json, expected) in cases {
            let msg: ServerMessage = serde_json::from_str(json).expect("测试 JSON 应可反序列化");
            assert_eq!(msg.type_name(), expected, "JSON: {json}");
        }
    }
}
