//! 协作功能状态管理

/// 协作连接状态
#[derive(Debug, Clone, Default)]
pub(crate) enum CollaborationStatus {
    #[default]
    Disconnected,
    Connecting,
}
