#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    Drag,
    Close,
    ToggleMaximize,
    Maximize,
    Minimize,
    /// 打开自定义精度对话框窗口
    OpenCustomPrecisionDialog,
    /// 打开加载确认对话框
    OpenLoadConfirmDialog {
        path: String,
        size_mb: f64,
    },
    /// 关闭自定义精度对话框窗口
    CloseCustomPrecisionDialog,
    /// 应用自定义精度设置 (numerator, denominator)
    ApplyCustomPrecision(u32, u32),
    /// 打开协作对话框窗口
    OpenCollaborationDialog,
    /// 关闭协作对话框窗口
    CloseCollaborationDialog,
    /// 连接协作服务器
    CollaborationConnect {
        host: String,
        port: u16,
        username: String,
        invite_code: Option<String>,
    },
    /// 创建协作房间
    CollaborationCreateRoom {
        name: String,
    },
    /// 加入协作房间
    CollaborationJoinRoom {
        invite_code: String,
    },
    /// 断开协作连接
    CollaborationDisconnect,
    /// 协作认证成功
    CollaborationAuthenticated {
        user_id: String,
        invite_code: String,
    },
    /// 协作房间创建成功
    CollaborationRoomCreated {
        room_name: String,
        invite_code: String,
    },
    /// 协作加入房间成功
    CollaborationRoomJoined {
        room_name: String,
        invite_code: String,
        user_count: usize,
    },
    /// 协作连接断开
    CollaborationDisconnected,
    /// 协作用户离开
    CollaborationUserLeft {
        user_id: String,
    },
    /// 协作鼠标位置更新
    CollaborationMouseUpdate {
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    },
    /// 协作音符更新（来自其他用户）
    CollaborationNoteUpdate {
        user_id: String,
        operation: String, // JSON string of NoteBatchOperation
    },
    /// 本地笔记更新（需要同步到其他用户）
    LocalNoteAdded {
        tick: f32,
        key: u16,
        length: f32,
        velocity: u8,
        channel: u8,
        track_index: usize,
    },
    /// 本地音符移动（需要同步到其他用户）
    LocalNoteMoved {
        tick: f32,
        key: u16,
        length: f32,
        tick_offset: f32,
        key_offset: i16,
        track_index: usize,
    },
    /// 打开音符变速对话框
    OpenSpeedChangeDialog,
    /// 关闭音符变速对话框
    CloseSpeedChangeDialog,
    /// 确认音符变速
    ConfirmSpeedChange(f32),
    /// 打开工程设置对话框
    OpenProjectSettingsDialog,
    /// 关闭工程设置对话框
    CloseProjectSettingsDialog,
    /// 应用工程设置
    ApplyProjectSettings {
        title: String,
        tempo: f64,
        copyright: String,
    },
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::Drag => "拖动".to_string(),
            Self::Close => "关闭".to_string(),
            Self::ToggleMaximize => "切换最大化".to_string(),
            Self::Maximize => "最大化".to_string(),
            Self::Minimize => "最小化".to_string(),
            Self::OpenCustomPrecisionDialog => "自定义精度".to_string(),
            Self::OpenLoadConfirmDialog { .. } => "加载确认".to_string(),
            Self::CloseCustomPrecisionDialog => "关闭自定义精度".to_string(),
            Self::ApplyCustomPrecision(_, _) => "应用精度设置".to_string(),
            Self::OpenCollaborationDialog => "协作".to_string(),
            Self::CloseCollaborationDialog => "关闭协作".to_string(),
            Self::CollaborationConnect { .. } => "连接协作服务器".to_string(),
            Self::CollaborationCreateRoom { .. } => "创建协作房间".to_string(),
            Self::CollaborationJoinRoom { .. } => "加入协作房间".to_string(),
            Self::CollaborationDisconnect => "断开协作连接".to_string(),
            Self::CollaborationAuthenticated { .. } => "协作认证成功".to_string(),
            Self::CollaborationRoomCreated { .. } => "房间创建成功".to_string(),
            Self::CollaborationRoomJoined { .. } => "已加入房间".to_string(),
            Self::CollaborationDisconnected => "协作已断开".to_string(),
            Self::CollaborationUserLeft { .. } => "用户离开".to_string(),
            Self::CollaborationMouseUpdate { .. } => "鼠标位置更新".to_string(),
            Self::CollaborationNoteUpdate { .. } => "音符更新".to_string(),
            Self::LocalNoteAdded { .. } => "本地音符已添加".to_string(),
            Self::LocalNoteMoved { .. } => "本地音符已移动".to_string(),
            Self::OpenSpeedChangeDialog => "音符变速".to_string(),
            Self::CloseSpeedChangeDialog => "关闭音符变速".to_string(),
            Self::ConfirmSpeedChange(_) => "确认变速".to_string(),
            Self::OpenProjectSettingsDialog => "工程设置".to_string(),
            Self::CloseProjectSettingsDialog => "关闭工程设置".to_string(),
            Self::ApplyProjectSettings { .. } => "应用工程设置".to_string(),
        }
    }
}
