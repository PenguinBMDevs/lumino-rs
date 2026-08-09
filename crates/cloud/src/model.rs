//! 云存储连接模型与状态

use serde::{Deserialize, Serialize};

/// 云存储协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CloudProtocol {
    /// FTP（文件传输协议，默认端口 21）
    #[default]
    Ftp,
    /// SFTP（SSH 文件传输协议，默认端口 22）
    Sftp,
    /// WebDAV（基于 HTTP 的分布式文件协议，默认端口 80/443）
    Webdav,
}

impl CloudProtocol {
    /// 协议默认端口
    pub fn default_port(self) -> u16 {
        match self {
            CloudProtocol::Ftp => 21,
            CloudProtocol::Sftp => 22,
            CloudProtocol::Webdav => 80,
        }
    }

    /// 协议显示名称
    pub fn display_name(self) -> &'static str {
        match self {
            CloudProtocol::Ftp => "FTP",
            CloudProtocol::Sftp => "SFTP",
            CloudProtocol::Webdav => "WebDAV",
        }
    }
}

impl std::fmt::Display for CloudProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// 云存储连接配置
///
/// 注意：`password_encrypted` 保存的是 AES-256-GCM 密文（base64 编码），
/// 明文密码不会持久化。`Debug` 实现隐藏密文，避免日志泄露凭证。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConnection {
    /// 连接唯一 ID（时间戳 + 随机后缀生成）
    pub id: String,
    /// 显示名称（用户自定义，如"我的 NAS"）
    pub name: String,
    /// 协议类型
    pub protocol: CloudProtocol,
    /// 服务器地址（域名或 IP）
    pub address: String,
    /// 端口（None = 协议默认端口）
    pub port: Option<u16>,
    /// 用户名
    pub username: String,
    /// 密码密文：base64(nonce || ciphertext)
    pub password_encrypted: String,
    /// 是否在应用启动时自动连接
    #[serde(default = "default_auto_connect")]
    pub auto_connect: bool,
    /// 默认根路径（WebDAV 常用 "/"；FTP/SFTP 为空表示登录后家目录）
    #[serde(default)]
    pub root_path: String,
}

fn default_auto_connect() -> bool {
    true
}

impl CloudConnection {
    /// 创建新连接（自动生成 ID）
    pub fn new(
        name: String,
        protocol: CloudProtocol,
        address: String,
        port: Option<u16>,
        username: String,
        password_encrypted: String,
        root_path: String,
    ) -> Self {
        let ts = chrono::Utc::now().timestamp_millis();
        let suffix: String = rand::random::<u16>().to_string();
        Self {
            id: format!("conn-{ts}-{suffix}"),
            name,
            protocol,
            address,
            port,
            username,
            password_encrypted,
            auto_connect: true,
            root_path,
        }
    }

    /// 实际连接端口（None 回退协议默认）
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| self.protocol.default_port())
    }
}

/// 云存储连接状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnState {
    /// 未连接（已配置但未连接/已断开）
    Disconnected,
    /// 连接中
    Connecting,
    /// 已连接
    Online,
    /// 连接失败（含原因）
    Failed(String),
}

impl ConnState {
    /// 是否处于可用状态
    pub fn is_online(&self) -> bool {
        matches!(self, ConnState::Online)
    }
}

/// 云存储中的文件/目录条目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudEntry {
    /// 条目名称（不含路径）
    pub name: String,
    /// 远程完整路径
    pub path: String,
    /// 是否为目录
    pub is_dir: bool,
    /// 文件大小（字节，目录为 0）
    pub size: u64,
    /// 修改时间（Unix 时间戳秒，未知为 None）
    pub modified: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_default_port() {
        assert_eq!(CloudProtocol::Ftp.default_port(), 21);
        assert_eq!(CloudProtocol::Sftp.default_port(), 22);
        assert_eq!(CloudProtocol::Webdav.default_port(), 80);
    }

    #[test]
    fn test_protocol_display_name() {
        assert_eq!(CloudProtocol::Ftp.display_name(), "FTP");
        assert_eq!(CloudProtocol::Sftp.display_name(), "SFTP");
        assert_eq!(CloudProtocol::Webdav.display_name(), "WebDAV");
    }

    #[test]
    fn test_effective_port_fallback() {
        let conn = CloudConnection::new(
            "测试".into(),
            CloudProtocol::Sftp,
            "example.com".into(),
            None,
            "user".into(),
            "abc".into(),
            String::new(),
        );
        assert_eq!(conn.effective_port(), 22);

        let conn2 = CloudConnection::new(
            "测试".into(),
            CloudProtocol::Sftp,
            "example.com".into(),
            Some(2222),
            "user".into(),
            "abc".into(),
            String::new(),
        );
        assert_eq!(conn2.effective_port(), 2222);
    }

    #[test]
    fn test_connection_id_unique() {
        let a = CloudConnection::new(
            "a".into(),
            CloudProtocol::Ftp,
            "h".into(),
            None,
            "u".into(),
            "p".into(),
            String::new(),
        );
        let b = CloudConnection::new(
            "b".into(),
            CloudProtocol::Ftp,
            "h".into(),
            None,
            "u".into(),
            "p".into(),
            String::new(),
        );
        assert_ne!(a.id, b.id, "连接 ID 必须唯一");
    }

    #[test]
    fn test_conn_state_is_online() {
        assert!(ConnState::Online.is_online());
        assert!(!ConnState::Disconnected.is_online());
        assert!(!ConnState::Connecting.is_online());
        assert!(!ConnState::Failed("x".into()).is_online());
    }
}
