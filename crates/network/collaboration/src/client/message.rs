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
    /// 项目状态更新
    ProjectUpdate {
        /// 项目更新内容
        update: crate::types::ProjectUpdate,
    },
    /// 选择同步（本地选择变更广播给其他客户端）
    Selection {
        /// 选择内容（JSON：{active, timestamp, fingerprints}）
        selection: serde_json::Value,
    },
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
        /// 房间信息（认证成功但未进房时为 null，例如仅登录或加房失败）
        #[serde(default)]
        room: Option<AuthRoomInfo>,
        /// 当前用户数据
        user: crate::types::UserData,
        /// 房间内用户列表
        users: Vec<crate::types::UserInfo>,
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
        // 2026-10-20（审计 §十一）：删除 `project_state`（房间全量状态快照）。
        // 它反序列化后从未被读取（`handlers.rs` 以 `..` 丢弃），是「房间内第二份
        // 音符数据」的载体；音符的唯一权威是各端的 `MidiDocument`。
        // 服务端仍可下发该字段——serde 会忽略未知字段，协议不会因此报错。
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
    /// 选择更新（来自其他用户的本地选择变更）
    Selection {
        /// 发起选择的用户 ID
        #[serde(rename = "userId")]
        user_id: crate::types::UserId,
        /// 选择内容（JSON：{active, timestamp, fingerprints}）
        selection: serde_json::Value,
    },
    /// 全量同步响应
    FullSync {
        // 2026-10-20（审计 §十一）：删除 `project_state`（理由同 `RoomJoined`）。
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

/// 认证响应中的房间信息
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
    /// 房主上传的工程名（协作工程同步用；可能为空）
    #[serde(default)]
    pub project_name: String,
    /// 房主工程文件哈希（lmpj 字节哈希；joiner 据此判断是否同一工程）
    #[serde(default)]
    pub project_hash: String,
}

#[cfg(test)]
mod tests {
    use super::ServerMessage;

    /// M1 回归守卫：服务端在「仅登录 / 加房失败」时发 `room: null`，
    /// 客户端必须能解析为 None，而不是把整个认证响应判为格式错误。
    #[test]
    fn test_authenticated_room_null_is_parsable() {
        let json = r#"{"type":"authenticated","userId":"u1","room":null,"user":{"id":"u1","username":"tester"},"users":[]}"#;
        let msg: ServerMessage =
            serde_json::from_str(json).expect("room:null 的认证响应必须可解析");
        match msg {
            ServerMessage::Authenticated { room, .. } => {
                assert!(room.is_none(), "room:null 应解析为 None");
            }
            other => panic!("应为 Authenticated，实际: {other:?}"),
        }
    }
}
