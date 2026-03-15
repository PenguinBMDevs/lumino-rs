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
        user: serde_json::Value,
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
        project_state: serde_json::Value,
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
        project_state: serde_json::Value,
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
