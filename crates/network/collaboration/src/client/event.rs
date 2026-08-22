//! 协作事件定义

/// 事件回调类型
pub type EventCallback = std::sync::Arc<dyn Fn(CollaborationEvent) + Send + Sync>;

/// 协作事件
#[derive(Debug, Clone)]
pub enum CollaborationEvent {
    /// 已连接到服务器
    Connected,
    /// 已断开连接
    Disconnected,
    /// 认证成功
    Authenticated {
        /// 当前用户 ID
        user_id: crate::types::UserId,
        /// 邀请码
        invite_code: crate::types::InviteCode,
    },
    /// 房间创建成功
    RoomCreated {
        /// 创建的房间信息
        room: crate::types::RoomInfo,
    },
    /// 已加入房间
    RoomJoined {
        /// 加入的房间信息
        room: crate::types::RoomInfo,
        /// 房间内的用户列表
        users: Vec<crate::types::UserInfo>,
    },
    /// 有新用户加入房间
    UserJoined {
        /// 新加入的用户信息
        user: crate::types::UserInfo,
    },
    /// 有用户离开房间
    UserLeft {
        /// 离开用户的 ID
        user_id: crate::types::UserId,
    },
    /// 鼠标位置更新
    MouseUpdate {
        /// 移动鼠标的用户 ID
        user_id: crate::types::UserId,
        /// 鼠标坐标
        position: crate::types::MousePosition,
        /// 用户头衔颜色
        color: String,
        /// 用户名
        username: String,
    },
    /// 音符批量操作
    NoteBatch {
        /// 发起操作的用户 ID
        user_id: crate::types::UserId,
        /// 音符批量操作内容
        operation: crate::types::NoteBatchOperation,
    },
    /// MIDI 事件
    MidiEvent {
        /// 发起事件的用户 ID
        user_id: crate::types::UserId,
        /// MIDI 事件内容
        event: crate::types::MidiEvent,
    },
    /// MIDI 事件批量更新
    MidiEventBatch {
        /// 发起事件的用户 ID
        user_id: crate::types::UserId,
        /// MIDI 事件列表
        events: Vec<crate::types::MidiEvent>,
    },
    /// 项目状态更新
    ProjectUpdate {
        /// 发起更新的用户 ID
        user_id: crate::types::UserId,
        /// 项目更新内容
        update: crate::types::ProjectUpdate,
    },
    /// 选择更新（来自其他用户的本地选择变更）
    Selection {
        /// 发起选择的用户 ID
        user_id: crate::types::UserId,
        /// 选择内容（JSON：{active, timestamp, fingerprints}）
        selection: serde_json::Value,
    },
    /// 全量同步
    FullSync {
        /// 同步到的用户列表
        users: Vec<crate::types::UserInfo>,
    },
    /// 错误事件
    Error {
        /// 错误描述信息
        message: String,
    },
}
