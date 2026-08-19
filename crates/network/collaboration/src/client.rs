//! 协作客户端
//!
//! 新架构：HTTP API + WebSocket (通过 RoomDurableObject)
//!
//! 子模块：
//! - `core`: 客户端核心定义和基础方法
//! - `connection`: 连接管理（WebSocket 连接、房间操作）
//! - `event`: 事件类型定义
//! - `handlers`: 服务器消息处理
//! - `message`: 消息类型定义
//! - `messaging`: 消息发送和接收
//! - `room`: 房间管理
//! - `state`: 客户端状态管理

// 子模块
/// 连接管理模块
pub mod connection;
/// 客户端核心模块
pub mod core;
/// 事件类型模块
pub mod event;
/// 服务器消息处理模块
pub mod handlers;
/// 消息类型模块
pub mod message;
/// 消息发送与接收模块
pub mod messaging;
/// 房间管理模块
pub mod room;
/// 客户端状态管理模块
pub mod state;

// 公开导出
pub use core::CollaborationClient;
pub use event::{CollaborationEvent, EventCallback};
pub use handlers::handle_server_message;
pub use message::{ClientMessage, ServerMessage};
pub use state::{ClientState, CollaborationSession};
