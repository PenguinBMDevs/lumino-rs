// ClientState 和 CollaborationSession 在 client/state.rs 定义，
// 这里统一 re-export 避免重复定义。
pub use crate::client::state::{ClientState, CollaborationSession};

/// 客户端配置
///
/// 注（DEBT-06 #123）：不包含自动重连配置。断连只发 `Disconnected` 事件，
/// 重连由上层（协作服务 / UI 的"重连"入口）显式发起——旧的
/// `auto_reconnect` / `max_reconnect_attempts` 字段从未被任何代码消费，
/// 属于"配置承诺 ≠ 行为"的信任债，已摘除。
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// 服务器主机地址
    pub server_host: String,
    /// 服务器端口
    pub server_port: u16,
    /// 用户名
    pub username: String,
    /// 密码（与注册/登录账户一致，用于 WebSocket 握手鉴权）
    pub password: String,
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
            password: String::new(),
        }
    }
}
