//! 协作客户端核心定义和基础方法

use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tracing::info;

use crate::Result;
use crate::http::HttpClient;

use super::{ClientMessage, ClientState, CollaborationEvent, CollaborationSession, EventCallback};

/// 协作客户端
pub struct CollaborationClient {
    pub(super) config: crate::types::ClientConfig,
    pub(super) state: Arc<RwLock<ClientState>>,
    pub(super) session: Arc<RwLock<CollaborationSession>>,
    pub(super) message_tx: mpsc::UnboundedSender<ClientMessage>,
    pub(super) message_rx: Option<mpsc::UnboundedReceiver<ClientMessage>>,
    pub(super) event_callback: Option<EventCallback>,
    pub(super) shutdown_tx: Option<mpsc::Sender<()>>,
    pub(super) http_client: HttpClient,
    pub(super) room_id: Option<String>,
}

impl CollaborationClient {
    /// 创建新客户端
    pub fn new(config: crate::types::ClientConfig) -> Self {
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let http_client = HttpClient::new(&config.server_host, config.server_port);

        Self {
            config,
            state: Arc::new(RwLock::new(ClientState::Disconnected)),
            session: Arc::new(RwLock::new(CollaborationSession::default())),
            message_tx,
            message_rx: Some(message_rx),
            event_callback: None,
            shutdown_tx: None,
            http_client,
            room_id: None,
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
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        *self.state.write().await = ClientState::Disconnected;
        info!("已断开连接");

        Ok(())
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

    /// 发送消息（内部方法）
    pub(super) async fn send_message(&self, msg: ClientMessage) -> Result<()> {
        let state = *self.state.read().await;
        if !matches!(
            state,
            ClientState::Connected | ClientState::Authenticated | ClientState::InRoom
        ) {
            return Err(format!("客户端未连接，当前状态: {:?}", state).into());
        }

        self.message_tx.send(msg).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 发射事件
    pub(super) fn emit_event(&self, event: CollaborationEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// 生成用户 ID
    pub(super) fn generate_user_id(&self) -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("user_{}_{}", timestamp, rand::random_u32())
    }
}

// 随机数生成模块
pub(super) mod rand {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    /// 生成随机 u32 值
    pub fn random_u32() -> u32 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        hasher.finish() as u32
    }
}
