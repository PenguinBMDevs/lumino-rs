//! 协作对话框状态

/// 协作视图状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollaborationViewState {
    #[default]
    /// 连接服务器界面（默认）
    Connect, // 连接服务器界面
    /// 正在连接中
    Connecting, // 正在连接中
    /// 创建/加入房间界面
    RoomActions, // 创建/加入房间界面
    /// 在房间内界面
    InRoom, // 在房间内界面
}

/// 协作对话框状态
#[derive(Debug, Clone)]
pub struct CollaborationDialogState {
    /// 对话框是否打开
    pub is_open: bool,
    /// 服务器地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: String,
    /// 用户名
    pub username: String,
    /// 密码（与注册/登录账户一致，用于 WebSocket 握手鉴权）
    pub password: String,
    /// 房间名称（创建房间用）
    pub room_name: String,
    /// 邀请码（加入房间用）
    pub invite_code: String,
    /// 当前视图状态
    pub view_state: CollaborationViewState,
    /// 连接状态
    pub connection_status: String,
}

impl Default for CollaborationDialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl CollaborationDialogState {
    /// 创建一个默认的协作对话框状态
    pub fn new() -> Self {
        Self {
            is_open: false,
            server_host: "localhost".to_string(),
            server_port: "3000".to_string(),
            username: "用户".to_string(),
            password: String::new(),
            room_name: "我的房间".to_string(),
            invite_code: String::new(),
            view_state: CollaborationViewState::Connect,
            connection_status: String::new(),
        }
    }

    /// 重置对话框状态为初始值（关闭、回到连接界面、清空状态文本）
    pub fn reset(&mut self) {
        self.is_open = false;
        self.view_state = CollaborationViewState::Connect;
        self.connection_status.clear();
    }
}
