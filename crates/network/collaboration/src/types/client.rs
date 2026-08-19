// ClientState 和 CollaborationSession 在 client/state.rs 定义，
// 这里统一 re-export 避免重复定义。
pub use crate::client::state::{ClientState, CollaborationSession};

/// 客户端配置
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// 服务器主机地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: u16,
    /// 用户名
    pub username: String,
    /// 是否自动重连
    pub auto_reconnect: bool,
    /// 最大重连尝试次数
    pub max_reconnect_attempts: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_millis() as u32;

        Self {
            server_host: "localhost".to_string(),
            server_port: 3000,
            username: format!("用户{}", seed % 10000),
            auto_reconnect: true,
            max_reconnect_attempts: 5,
        }
    }
}
