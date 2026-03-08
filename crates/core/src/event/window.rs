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
    /// 协作鼠标位置更新
    CollaborationMouseUpdate {
        user_id: String,
        x: f32,
        y: f32,
        color: String,
    },
    /// 协作音符更新
    CollaborationNoteUpdate {
        user_id: String,
        operation: String, // JSON string of NoteBatchOperation
    },
}
