//! 协作客户端
//!
//! 该模块已拆分为以下子模块：
//! - `message`: 客户端/服务器消息定义
//! - `event`: 协作事件定义
//! - `connection`: 连接管理
//! - `handlers`: 服务器消息处理器

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{debug, error, info};

use crate::HEARTBEAT_INTERVAL_MS;
use crate::types::*;

pub mod connection;
pub mod event;
pub mod handlers;
pub mod message;

pub use event::{CollaborationEvent, EventCallback};
pub use handlers::handle_server_message;
pub use message::{ClientMessage, ServerMessage};

/// 客户端状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    InRoom,
    Error,
}

/// 协作会话信息
#[derive(Debug, Clone, Default)]
pub struct CollaborationSession {
    pub current_user_id: Option<UserId>,
    pub invite_code: Option<InviteCode>,
    pub current_room: Option<RoomInfo>,
    pub remote_users: std::collections::HashMap<UserId, RemoteUser>,
}

/// 协作客户端
pub struct CollaborationClient {
    config: ClientConfig,
    state: Arc<RwLock<ClientState>>,
    session: Arc<RwLock<CollaborationSession>>,
    message_tx: mpsc::UnboundedSender<ClientMessage>,
    message_rx: Arc<Mutex<mpsc::UnboundedReceiver<ClientMessage>>>,
    event_callback: Option<EventCallback>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl CollaborationClient {
    /// 创建新客户端
    pub fn new(config: ClientConfig) -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            session: Arc::new(RwLock::new(CollaborationSession::default())),
            message_tx,
            message_rx: Arc::new(Mutex::new(message_rx)),
            event_callback: None,
            shutdown_tx: None,
        }
    }

    /// 设置事件回调
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(CollaborationEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Arc::new(callback));
    }

    /// 断开连接
    pub async fn disconnect(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        *self.state.write().await = ClientState::Disconnected;
        info!("已断开连接");

        Ok(())
    }

    /// 创建房间
    pub fn create_room(&self, name: String) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::CreateRoom { name })
    }

    /// 加入房间
    pub fn join_room(&self, invite_code: String) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::JoinRoom { invite_code })
    }

    /// 离开房间
    pub fn leave_room(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::LeaveRoom)
    }

    /// 发送鼠标位置
    pub fn send_mouse_position(
        &self,
        position: MousePosition,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MouseMove { position })
    }

    /// 发送音符批量操作
    pub fn send_note_batch(
        &self,
        operation: NoteBatchOperation,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::NoteBatch { notes: operation })
    }

    /// 发送MIDI事件
    pub fn send_midi_event(&self, event: MidiEvent) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MidiEvent { event })
    }

    /// 发送MIDI事件批量
    pub fn send_midi_event_batch(
        &self,
        events: Vec<MidiEvent>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::MidiEventBatch { events })
    }

    /// 发送项目更新
    pub fn send_project_update(
        &self,
        update: ProjectUpdate,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::ProjectUpdate { update })
    }

    /// 请求同步
    pub fn request_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_message(ClientMessage::RequestSync)
    }

    /// 获取当前状态
    pub async fn state(&self) -> ClientState {
        *self.state.read().await
    }

    /// 获取当前会话
    pub async fn session(&self) -> CollaborationSession {
        self.session.read().await.clone()
    }

    /// 是否已连接
    pub async fn is_connected(&self) -> bool {
        matches!(
            *self.state.read().await,
            ClientState::Connected | ClientState::Authenticated | ClientState::InRoom
        )
    }

    /// 是否在房间中
    pub async fn is_in_room(&self) -> bool {
        *self.state.read().await == ClientState::InRoom
    }

    // 内部方法
    fn send_message(&self, msg: ClientMessage) -> Result<(), Box<dyn std::error::Error>> {
        info!("排队发送消息: {:?}", msg);
        self.message_tx.send(msg).map_err(|e| e.to_string().into())
    }

    fn emit_event(&self, event: CollaborationEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }
}
