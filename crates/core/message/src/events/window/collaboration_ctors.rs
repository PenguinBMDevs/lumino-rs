use super::*;

impl Event {
    // ── 协作构造函数（直接构造 collaboration::Event） ──

    /// 构造协作连接事件
    pub fn collaboration_connect(
        host: String,
        port: u16,
        username: String,
        password: String,
        invite_code: Option<String>,
    ) -> Self {
        Self::Collaboration(collaboration::Event::Connect {
            host,
            port,
            username,
            password,
            invite_code,
        })
    }
    /// 构造协作创建房间事件
    pub fn collaboration_create_room(name: String) -> Self {
        Self::Collaboration(collaboration::Event::CreateRoom { name })
    }
    /// 构造协作加入房间事件
    pub fn collaboration_join_room(invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::JoinRoom { invite_code })
    }
    /// 构造协作断开事件
    pub const fn collaboration_disconnect() -> Self {
        Self::Collaboration(collaboration::Event::Disconnect)
    }
    /// 构造协作认证成功事件
    pub fn collaboration_authenticated(user_id: String, invite_code: String) -> Self {
        Self::Collaboration(collaboration::Event::Authenticated {
            user_id,
            invite_code,
        })
    }
    /// 构造协作房间已创建事件
    pub fn collaboration_room_created(
        room_name: String,
        invite_code: String,
        project_name: String,
        project_hash: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::RoomCreated {
            room_name,
            invite_code,
            project_name,
            project_hash,
        })
    }
    /// 构造协作房间已加入事件
    pub fn collaboration_room_joined(
        room_name: String,
        invite_code: String,
        user_count: usize,
        project_name: String,
        project_hash: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::RoomJoined {
            room_name,
            invite_code,
            user_count,
            project_name,
            project_hash,
        })
    }
    /// 构造协作已断开事件
    pub const fn collaboration_disconnected() -> Self {
        Self::Collaboration(collaboration::Event::Disconnected)
    }
    /// 构造协作连接失败事件
    pub fn collaboration_connect_failed(reason: String) -> Self {
        Self::Collaboration(collaboration::Event::ConnectFailed { reason })
    }
    /// 构造协作用户离开事件
    pub fn collaboration_user_left(user_id: String) -> Self {
        Self::Collaboration(collaboration::Event::UserLeft { user_id })
    }
    /// 构造协作远端鼠标移动事件
    pub fn collaboration_mouse_update(
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) -> Self {
        Self::Collaboration(collaboration::Event::MouseUpdate {
            user_id,
            x,
            y,
            color,
            username,
        })
    }
    /// 构造协作音符更新事件
    pub fn collaboration_note_update(user_id: String, operation: String) -> Self {
        Self::Collaboration(collaboration::Event::NoteUpdate { user_id, operation })
    }
    /// 构造协作工程更新事件
    pub fn collaboration_project_update(user_id: String, update: String) -> Self {
        Self::Collaboration(collaboration::Event::ProjectUpdate { user_id, update })
    }
    /// 构造远端选择更新事件
    pub fn collaboration_selection(user_id: String, selection: String, color: String) -> Self {
        Self::Collaboration(collaboration::Event::Selection {
            user_id,
            selection,
            color,
        })
    }
}
