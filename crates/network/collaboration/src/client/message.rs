//! 协作客户端消息定义

/// 客户端到服务器的消息
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ClientMessage {
    Auth {
        username: String,
    },
    CreateRoom {
        name: String,
    },
    JoinRoom {
        #[serde(rename = "inviteCode")]
        invite_code: String,
    },
    LeaveRoom,
    MouseMove {
        position: crate::types::MousePosition,
    },
    NoteBatch {
        notes: crate::types::NoteBatchOperation,
    },
    MidiEvent {
        event: crate::types::MidiEvent,
    },
    MidiEventBatch {
        events: Vec<crate::types::MidiEvent>,
    },
    ProjectUpdate {
        update: crate::types::ProjectUpdate,
    },
    RequestSync,
    Ping {
        timestamp: u64,
    },
}

/// 服务器到客户端的消息
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub enum ServerMessage {
    Authenticated {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        room: AuthRoomInfo,
        user: crate::types::UserData,
        users: Vec<crate::types::UserInfo>,
    },
    AuthSuccess {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        #[serde(rename = "inviteCode")]
        invite_code: crate::types::InviteCode,
    },
    AuthError {
        error: String,
    },
    RoomCreated {
        room: crate::types::RoomInfo,
    },
    RoomJoined {
        room: crate::types::RoomInfo,
        users: Vec<crate::types::UserInfo>,
        #[serde(rename = "projectState")]
        project_state: crate::types::ProjectState,
    },
    RoomError {
        error: String,
    },
    UserJoined {
        user: crate::types::UserInfo,
    },
    UserLeft {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
    },
    MouseUpdate {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        username: String,
        position: crate::types::MousePosition,
        color: String,
    },
    NoteBatchUpdate {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        operation: crate::types::NoteBatchOperation,
    },
    MidiEventUpdate {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        event: crate::types::MidiEvent,
    },
    MidiEventBatchUpdate {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        events: Vec<crate::types::MidiEvent>,
    },
    ProjectStateUpdate {
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        update: crate::types::ProjectUpdate,
    },
    FullSync {
        #[serde(rename = "projectState")]
        project_state: crate::types::ProjectState,
        users: Vec<crate::types::UserInfo>,
    },
    Pong {
        timestamp: u64,
        #[serde(rename = "serverTime")]
        server_time: u64,
    },
    Error {
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
    pub id: String,
    #[serde(rename = "inviteCode")]
    pub invite_code: crate::types::InviteCode,
    pub name: String,
    #[serde(rename = "hostId")]
    pub host_id: crate::types::UserId,
    #[serde(rename = "userCount")]
    pub user_count: u32,
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
