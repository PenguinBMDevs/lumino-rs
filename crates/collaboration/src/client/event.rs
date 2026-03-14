//! 协作事件定义

/// 事件回调类型
pub type EventCallback = std::sync::Arc<dyn Fn(CollaborationEvent) + Send + Sync>;

/// 协作事件
#[derive(Debug, Clone)]
pub enum CollaborationEvent {
    Connected,
    Disconnected,
    Authenticated {
        user_id: crate::types::UserId,
        invite_code: crate::types::InviteCode,
    },
    RoomCreated {
        room: crate::types::RoomInfo,
    },
    RoomJoined {
        room: crate::types::RoomInfo,
        users: Vec<crate::types::UserInfo>,
    },
    UserJoined {
        user: crate::types::UserInfo,
    },
    UserLeft {
        user_id: crate::types::UserId,
    },
    MouseUpdate {
        user_id: crate::types::UserId,
        position: crate::types::MousePosition,
        color: String,
    },
    NoteBatch {
        user_id: crate::types::UserId,
        operation: crate::types::NoteBatchOperation,
    },
    MidiEvent {
        user_id: crate::types::UserId,
        event: crate::types::MidiEvent,
    },
    MidiEventBatch {
        user_id: crate::types::UserId,
        events: Vec<crate::types::MidiEvent>,
    },
    ProjectUpdate {
        user_id: crate::types::UserId,
        update: crate::types::ProjectUpdate,
    },
    FullSync {
        users: Vec<crate::types::UserInfo>,
    },
    Error {
        message: String,
    },
}
