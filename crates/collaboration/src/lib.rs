/**
 * Lumino 协作客户端
 *
 * 用于连接 Node.js 协作服务器，实现多人在线协作编辑
 */
pub mod client;
pub mod handlers;
pub mod types;

pub use client::{CollaborationClient, CollaborationEvent};
pub use types::*;

/// 默认服务器端口
pub const DEFAULT_SERVER_PORT: u16 = 3000;

/// 心跳间隔（毫秒）
pub const HEARTBEAT_INTERVAL_MS: u64 = 25000;

/// 重连间隔（毫秒）
pub const RECONNECT_INTERVAL_MS: u64 = 5000;
