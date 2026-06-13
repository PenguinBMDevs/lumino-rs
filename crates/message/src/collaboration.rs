//! 协作动作定义
//!
//! 从 Message 枚举中提取的协作相关动作，减少 Message 枚举的变体数量。

/// 协作动作
#[derive(Debug, Clone)]
pub enum CollaborationAction {
    /// 打开协作对话框
    OpenDialog,
    /// 关闭协作对话框
    CloseDialog,
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
    /// 协作服务器地址变更
    HostChanged(String),
    /// 协作服务器端口变更
    PortChanged(String),
    /// 协作用户名变更
    UsernameChanged(String),
    /// 协作房间名称变更
    RoomNameChanged(String),
    /// 协作邀请码变更
    InviteCodeChanged(String),
    /// 协作复制邀请码到剪贴板
    CopyInviteCode,
    /// 协作远端鼠标移动
    RemoteMouseMoved {
        user_id: std::sync::Arc<str>,
        x: f32,
        y: f32,
        color: std::sync::Arc<str>,
        username: std::sync::Arc<str>,
    },
    /// 协作用户离开
    RemoteUserLeft { user_id: std::sync::Arc<str> },
    /// 协作远端音符更新
    RemoteNoteUpdate { operation: String },
}
