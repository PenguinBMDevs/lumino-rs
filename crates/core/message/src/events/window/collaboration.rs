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
    /// 协作工程更新（来自其他用户，如音轨变更）
    ProjectUpdate {
        user_id: String,
        update: String, // JSON string of ProjectUpdate
    },
}
