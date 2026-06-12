//! 协作相关事件

#[derive(Debug, Clone)]
pub enum Event {
    /// 连接协作服务器
    Connect {
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    },
    /// 创建协作房间
    CreateRoom { name: String },
    /// 加入协作房间
    JoinRoom { invite_code: String },
    /// 断开协作连接
    Disconnect,
    /// 协作认证成功
    Authenticated {
        user_id: String,
        invite_code: String,
    },
    /// 协作房间创建成功
    RoomCreated {
        room_name: String,
        invite_code: String,
    },
    /// 协作加入房间成功
    RoomJoined {
        room_name: String,
        invite_code: String,
        user_count: usize,
    },
    /// 协作连接断开
    Disconnected,
    /// 协作用户离开
    UserLeft { user_id: String },
    /// 协作鼠标位置更新
    MouseUpdate {
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    },
    /// 协作音符更新（来自其他用户）
    NoteUpdate {
        user_id: String,
        operation: String, // JSON string of NoteBatchOperation
    },
}

impl Event {
    pub fn display_name(&self) -> String {
        match self {
            Self::Connect { .. } => "连接协作服务器".to_string(),
            Self::CreateRoom { .. } => "创建协作房间".to_string(),
            Self::JoinRoom { .. } => "加入协作房间".to_string(),
            Self::Disconnect => "断开协作连接".to_string(),
            Self::Authenticated { .. } => "协作认证成功".to_string(),
            Self::RoomCreated { .. } => "房间创建成功".to_string(),
            Self::RoomJoined { .. } => "已加入房间".to_string(),
            Self::Disconnected => "协作已断开".to_string(),
            Self::UserLeft { .. } => "用户离开".to_string(),
            Self::MouseUpdate { .. } => "鼠标位置更新".to_string(),
            Self::NoteUpdate { .. } => "音符更新".to_string(),
        }
    }

    pub fn connect(host: String, port: u16, username: String, invite_code: Option<String>) -> Self {
        Self::Connect {
            host,
            port,
            username,
            invite_code,
        }
    }
    pub fn create_room(name: String) -> Self {
        Self::CreateRoom { name }
    }
    pub fn join_room(invite_code: String) -> Self {
        Self::JoinRoom { invite_code }
    }
    pub const fn disconnect() -> Self {
        Self::Disconnect
    }
    pub fn authenticated(user_id: String, invite_code: String) -> Self {
        Self::Authenticated {
            user_id,
            invite_code,
        }
    }
    pub fn room_created(room_name: String, invite_code: String) -> Self {
        Self::RoomCreated {
            room_name,
            invite_code,
        }
    }
    pub fn room_joined(room_name: String, invite_code: String, user_count: usize) -> Self {
        Self::RoomJoined {
            room_name,
            invite_code,
            user_count,
        }
    }
    pub const fn disconnected() -> Self {
        Self::Disconnected
    }
    pub fn user_left(user_id: String) -> Self {
        Self::UserLeft { user_id }
    }
    pub fn mouse_update(user_id: String, x: f32, y: f32, color: String, username: String) -> Self {
        Self::MouseUpdate {
            user_id,
            x,
            y,
            color,
            username,
        }
    }
    pub fn note_update(user_id: String, operation: String) -> Self {
        Self::NoteUpdate { user_id, operation }
    }
}
