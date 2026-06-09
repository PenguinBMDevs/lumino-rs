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

    // ── 构造函数（替代 event! 宏，IDE 友好） ──

    pub const fn drag() -> Self { Self::Drag }
    pub const fn close() -> Self { Self::Close }
    pub const fn toggle_maximize() -> Self { Self::ToggleMaximize }
    pub const fn maximize() -> Self { Self::Maximize }
    pub const fn minimize() -> Self { Self::Minimize }
    pub const fn open_custom_precision_dialog() -> Self { Self::OpenCustomPrecisionDialog }
    pub const fn close_custom_precision_dialog() -> Self { Self::CloseCustomPrecisionDialog }
    pub const fn apply_custom_precision(numerator: u32, denominator: u32) -> Self { Self::ApplyCustomPrecision(numerator, denominator) }
    pub const fn open_collaboration_dialog() -> Self { Self::OpenCollaborationDialog }
    pub const fn close_collaboration_dialog() -> Self { Self::CloseCollaborationDialog }
    pub fn collaboration_connect(host: String, port: u16, username: String, invite_code: Option<String>) -> Self {
        Self::CollaborationConnect { host, port, username, invite_code }
    }
    pub fn collaboration_create_room(name: String) -> Self { Self::CollaborationCreateRoom { name } }
    pub fn collaboration_join_room(invite_code: String) -> Self { Self::CollaborationJoinRoom { invite_code } }
    pub const fn collaboration_disconnect() -> Self { Self::CollaborationDisconnect }
    pub fn collaboration_authenticated(user_id: String, invite_code: String) -> Self {
        Self::CollaborationAuthenticated { user_id, invite_code }
    }
    pub fn collaboration_room_created(room_name: String, invite_code: String) -> Self {
        Self::CollaborationRoomCreated { room_name, invite_code }
    }
    pub fn collaboration_room_joined(room_name: String, invite_code: String, user_count: usize) -> Self {
        Self::CollaborationRoomJoined { room_name, invite_code, user_count }
    }
    pub const fn collaboration_disconnected() -> Self { Self::CollaborationDisconnected }
    pub fn collaboration_user_left(user_id: String) -> Self { Self::CollaborationUserLeft { user_id } }
    pub fn collaboration_mouse_update(user_id: String, x: f32, y: f32, color: String, username: String) -> Self {
        Self::CollaborationMouseUpdate { user_id, x, y, color, username }
    }
    pub fn collaboration_note_update(user_id: String, operation: String) -> Self {
        Self::CollaborationNoteUpdate { user_id, operation }
    }
    pub fn local_note_added(tick: f32, key: u16, length: f32, velocity: u8, channel: u8, track_index: usize) -> Self {
        Self::LocalNoteAdded { tick, key, length, velocity, channel, track_index }
    }
    pub fn local_note_moved(tick: f32, key: u16, length: f32, tick_offset: f32, key_offset: i16, track_index: usize) -> Self {
        Self::LocalNoteMoved { tick, key, length, tick_offset, key_offset, track_index }
    }
    pub const fn open_speed_change_dialog() -> Self { Self::OpenSpeedChangeDialog }
    pub const fn close_speed_change_dialog() -> Self { Self::CloseSpeedChangeDialog }
    pub const fn confirm_speed_change(factor: f32) -> Self { Self::ConfirmSpeedChange(factor) }
    pub const fn open_project_settings_dialog() -> Self { Self::OpenProjectSettingsDialog }
    pub const fn close_project_settings_dialog() -> Self { Self::CloseProjectSettingsDialog }
    pub fn apply_project_settings(title: String, tempo: f64, copyright: String) -> Self {
        Self::ApplyProjectSettings { title, tempo, copyright }
    }
}
