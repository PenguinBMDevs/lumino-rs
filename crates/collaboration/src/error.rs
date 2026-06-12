//! Collaboration 错误类型定义

use thiserror::Error;

/// 协作模块错误类型
#[derive(Error, Debug)]
pub enum CollaborationError {
    /// HTTP 请求错误
    #[error("HTTP 请求失败: {0}")]
    Http(#[from] reqwest::Error),

    /// WebSocket 连接错误
    #[error("WebSocket 错误: {0}")]
    WebSocket(String),

    /// WebSocket 协议错误
    #[error("WebSocket 协议错误: {0}")]
    WsProtocol(#[from] tokio_tungstenite::tungstenite::Error),

    /// JSON 序列化/反序列化错误
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),

    /// 连接超时
    #[error("连接超时（{0}秒）")]
    Timeout(u64),

    /// 房间未找到
    #[error("房间未找到")]
    RoomNotFound,

    /// 未连接
    #[error("未连接到服务器")]
    NotConnected,

    /// 认证失败
    #[error("认证失败: {0}")]
    AuthFailed(String),

    /// 其他错误
    #[error("{0}")]
    Other(String),
}

/// 协作模块结果类型别名
pub type Result<T> = std::result::Result<T, CollaborationError>;

impl From<Box<dyn std::error::Error>> for CollaborationError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        CollaborationError::Other(err.to_string())
    }
}
