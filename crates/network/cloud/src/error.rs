//! 云存储错误类型

use thiserror::Error;

/// 云存储操作错误
#[derive(Error, Debug)]
pub enum CloudError {
    /// IO 错误（本地文件读写、网络连接）
    #[error("{}: {}", lumino_core::i18n::tr("IO 错误"), .0)]
    Io(#[from] std::io::Error),

    /// 连接失败（网络不可达、握手失败）
    #[error("{}: {}", lumino_core::i18n::tr("连接失败"), .0)]
    Connect(String),

    /// 认证失败（用户名/密码错误）
    #[error("{}: {}", lumino_core::i18n::tr("认证失败"), .0)]
    Auth(String),

    /// 配置错误（配置不存在、格式错误）
    #[error("{}: {}", lumino_core::i18n::tr("配置错误"), .0)]
    Config(String),

    /// 加密/解密错误
    #[error("{}: {}", lumino_core::i18n::tr("加密错误"), .0)]
    Crypto(String),

    /// 协议层错误（协议命令失败、响应异常）
    #[error("{}: {}", lumino_core::i18n::tr("协议错误"), .0)]
    Protocol(String),

    /// 未连接（操作时连接不存在或已断开）
    #[error("{}: {}", lumino_core::i18n::tr("未连接"), .0)]
    NotConnected(String),

    /// 通用操作失败
    #[error("{}: {}", lumino_core::i18n::tr("操作失败"), .0)]
    Operation(String),
}

/// 云存储操作结果类型别名
pub type Result<T> = std::result::Result<T, CloudError>;
