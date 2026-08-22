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
        /// 服务器主机地址
        host: String,
        /// 服务器端口
        port: u16,
        /// 协作用户名
        username: String,
        /// 密码（与注册/登录账户一致，用于 WebSocket 握手鉴权）
        password: String,
        /// 邀请码（加入已有房间时提供）
        invite_code: Option<String>,
    },
    /// 创建协作房间
    CreateRoom {
        /// 房间名称
        name: String,
    },
    /// 加入协作房间
    JoinRoom {
        /// 房间邀请码
        invite_code: String,
    },
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
    /// 协作密码变更（与注册/登录账户一致，用于 WebSocket 握手鉴权）
    PasswordChanged(String),
    /// 协作复制邀请码到剪贴板
    CopyInviteCode,
    /// 协作远端鼠标移动
    RemoteMouseMoved {
        /// 远端用户 ID
        user_id: std::sync::Arc<str>,
        /// 鼠标横向坐标
        x: f32,
        /// 鼠标纵向坐标
        y: f32,
        /// 远端用户颜色
        color: std::sync::Arc<str>,
        /// 远端用户名
        username: std::sync::Arc<str>,
    },
    /// 协作用户离开
    RemoteUserLeft {
        /// 离开的用户 ID
        user_id: std::sync::Arc<str>,
    },
    /// 协作远端音符更新
    RemoteNoteUpdate {
        /// 更新操作描述
        operation: String,
    },
    /// 协作远端选择更新
    RemoteSelection {
        /// 远端用户 ID
        user_id: std::sync::Arc<str>,
        /// 选择内容（JSON 字符串：{active, timestamp, fingerprints}）
        selection: String,
        /// 远端用户颜色（hex 字符串）
        color: std::sync::Arc<str>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collaboration_action_variants() {
        let action = CollaborationAction::OpenDialog;
        assert!(matches!(action, CollaborationAction::OpenDialog));

        let action = CollaborationAction::Disconnect;
        assert!(matches!(action, CollaborationAction::Disconnect));

        let action = CollaborationAction::Connect {
            host: "localhost".to_string(),
            port: 3000,
            username: "test".to_string(),
            password: "test".to_string(),
            invite_code: None,
        };
        assert!(matches!(action, CollaborationAction::Connect { .. }));
    }
}
