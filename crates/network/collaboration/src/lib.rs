//! Lumino 协作客户端
//!
//! 用于连接协作服务器，实现多人在线协作编辑。
//! 提供 WebSocket 消息通信、HTTP API 客户端以及覆盖层（Overlay）增量同步等能力。

/// 协作客户端模块
pub mod client;
/// 错误类型模块
pub mod error;
/// HTTP API 客户端模块
pub mod http;
/// 覆盖层增量同步模块
pub mod overlay;
/// 共享类型模块
pub mod types;

pub use client::{ClientMessage, CollaborationClient, CollaborationEvent, ServerMessage};
pub use error::{CollaborationError, Result};
pub use types::*;

/// 默认服务器端口
pub const DEFAULT_SERVER_PORT: u16 = 3000;

/// 心跳间隔（毫秒）
pub const HEARTBEAT_INTERVAL_MS: u64 = 25000;

/// 重连间隔（毫秒）
pub const RECONNECT_INTERVAL_MS: u64 = 5000;
