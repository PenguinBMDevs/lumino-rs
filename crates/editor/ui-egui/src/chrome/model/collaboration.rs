//! 协作对话框模型（四视图）。

/// 协作对话框：标题 / 关闭 / 当前视图。
#[derive(Debug, Clone, Default)]
pub struct CollaborationModel {
    /// 顶部是否有 30px 旧版 自制标题栏（`!use_native_titlebar`）。
    pub titlebar: bool,
    /// 标题「多人协作」（20px）。
    pub title: String,
    /// 关闭按钮（12px）。
    pub close: String,
    /// 当前视图。
    pub view: CollaborationViewModel,
}

/// 四种视图。
#[derive(Debug, Clone)]
pub enum CollaborationViewModel {
    /// 连接服务器（默认）。
    Connect(CollaborationConnectView),
    /// 正在连接。
    Connecting(CollaborationConnectingView),
    /// 创建 / 加入房间。
    RoomActions(CollaborationRoomActionsView),
    /// 已进入房间。
    InRoom(CollaborationInRoomView),
}

/// 连接视图字段。
#[derive(Debug, Clone, Default)]
pub struct CollaborationConnectView {
    /// 服务器地址。
    pub host: String,
    /// 地址占位。
    pub host_hint: String,
    /// 端口。
    pub port: String,
    /// 端口占位。
    pub port_hint: String,
    /// 用户名。
    pub username: String,
    /// 用户名占位。
    pub username_hint: String,
    /// 密码（掩码显示）。
    pub password: String,
    /// 密码占位。
    pub password_hint: String,
    /// 邀请码（可选）。
    pub invite: String,
    /// 邀请码占位。
    pub invite_hint: String,
    /// 连接按钮。
    pub connect: String,
    /// 连接状态提示（空串 = 不显示）。
    pub status: String,
}

/// 连接中视图。
#[derive(Debug, Clone, Default)]
pub struct CollaborationConnectingView {
    /// 状态行（16px 主色）。
    pub status: String,
    /// 提示行（14px）。
    pub hint: String,
}

/// 房间操作视图。
#[derive(Debug, Clone, Default)]
pub struct CollaborationRoomActionsView {
    /// 房间名称。
    pub room_name: String,
    /// 房间名占位。
    pub room_hint: String,
    /// 创建房间按钮。
    pub create: String,
    /// 「- 或 -」分隔文案。
    pub or: String,
    /// 邀请码。
    pub invite: String,
    /// 邀请码占位。
    pub invite_hint: String,
    /// 加入房间按钮。
    pub join: String,
}

/// 房间内视图。
#[derive(Debug, Clone, Default)]
pub struct CollaborationInRoomView {
    /// 「房间: {name}」（16px）。
    pub room_line: String,
    /// 「邀请码: 」标签（14px）。
    pub invite_label: String,
    /// 邀请码本体（14px 主色）。
    pub invite_code: String,
    /// 复制邀请码按钮（12px）。
    pub copy: String,
    /// 断开连接按钮（14px、危险色）。
    pub disconnect: String,
}

impl Default for CollaborationViewModel {
    fn default() -> Self {
        Self::Connect(CollaborationConnectView::default())
    }
}
