//! 协作客户端核心定义和基础方法

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tracing::info;

use crate::Result;
use crate::http::HttpClient;

use super::{
    ClientMessage, ClientState, ClientStateCell, CollaborationEvent, CollaborationSession,
    EventCallback,
};

/// 协作客户端
pub struct CollaborationClient {
    pub(super) config: crate::types::ClientConfig,
    pub(super) state: Arc<ClientStateCell>,
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
            state: Arc::new(ClientStateCell::new()),
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
    ///
    /// `shutdown_tx` 为 `mpsc::Sender`（有界但容量充足），使用 `try_send` 避免异步等待；
    /// 接收端关闭（任务已退出）时静默忽略即可。
    pub fn disconnect(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take()
            && tx.try_send(()).is_err()
        {
            tracing::debug!("协作关闭信号接收端已关闭，忽略");
        }

        self.state.set(ClientState::Disconnected);
        info!("已断开连接");

        Ok(())
    }

    /// 获取当前状态（无锁）
    pub fn state(&self) -> ClientState {
        self.state.get()
    }

    /// 获取当前会话
    pub async fn session(&self) -> CollaborationSession {
        self.session.read().await.clone()
    }

    /// 是否已连接（无锁）
    pub fn is_connected(&self) -> bool {
        self.state.is_active()
    }

    /// 入队业务消息（内部方法，同步）
    ///
    /// 仅校验状态是否处于活动态，消息通过 `mpsc` 无界通道转交后台发送循环，
    /// 不在此处执行任何异步 I/O，因此可以在 UI 线程等任意上下文调用。
    pub(super) fn enqueue_message(&self, msg: ClientMessage) -> Result<()> {
        if !self.state.is_active() {
            return Err(crate::CollaborationError::Other(format!(
                "客户端未连接，当前状态: {:?}",
                self.state.get()
            )));
        }

        self.message_tx
            .send(msg)
            .map_err(|e| crate::CollaborationError::Other(e.to_string()))?;
        Ok(())
    }

    /// 发射事件
    pub(super) fn emit_event(&self, event: CollaborationEvent) {
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// 生成用户 ID
    /// 使用时间戳 + PID + 单调计数器保证唯一性，不依赖加密安全随机数
    pub(super) fn generate_user_id(&self) -> String {
        static ID_COUNTER: AtomicU32 = AtomicU32::new(0);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("user_{}_{}_{}", timestamp, std::process::id(), counter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_user_id_format() {
        let client = CollaborationClient::new(crate::types::ClientConfig::default());
        let id = client.generate_user_id();
        // 格式: user_{timestamp}_{pid}_{counter}
        assert!(id.starts_with("user_"), "ID should start with 'user_'");
        let parts: Vec<&str> = id.split('_').collect();
        assert_eq!(
            parts.len(),
            4,
            "ID should have 4 underscore-separated parts"
        );
        // 验证第二部分是数字（时间戳）
        assert!(
            parts[1].parse::<u128>().is_ok(),
            "timestamp should be numeric"
        );
        // 验证第三部分是数字（PID）
        assert!(parts[2].parse::<u32>().is_ok(), "PID should be numeric");
        // 验证第四部分是数字（计数器）
        assert!(parts[3].parse::<u32>().is_ok(), "counter should be numeric");
    }

    #[test]
    fn test_generate_user_id_uniqueness() {
        let client = CollaborationClient::new(crate::types::ClientConfig::default());
        let id1 = client.generate_user_id();
        let id2 = client.generate_user_id();
        // 单调计数器保证同一进程中连续调用生成的 ID 不同
        assert_ne!(id1, id2, "consecutive IDs should be different");
    }

    #[test]
    fn test_client_initial_state() {
        let client = CollaborationClient::new(crate::types::ClientConfig::default());
        assert_eq!(client.state(), ClientState::Disconnected);
    }

    #[test]
    fn test_client_new_creates_channel() {
        let client = CollaborationClient::new(crate::types::ClientConfig::default());
        assert!(
            client.message_rx.is_some(),
            "message_rx should be Some after creation"
        );
        // shutdown_tx 初始为 None（需由 connect 方法设置）
        assert!(
            client.shutdown_tx.is_none(),
            "shutdown_tx should be None initially"
        );
    }
}
