// DMS 错误类型定义

use thiserror::Error;
/// DMS 操作错误
#[derive(Error, Debug)]
pub enum DmsError {
    /// 无效的魔数
    #[error("无效的 DMS 文件标识")]
    InvalidMagic,

    /// 意外的流结束
    #[error("意外的数据流结束")]
    UnexpectedEof,

    /// 文件损坏
    #[error("文件可能已损坏: {0}")]
    Corrupted(String),

    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// 无效的节点类型
    #[error("无效的节点类型: {0}")]
    InvalidNodeType(u64),

    /// 不支持的数据类型
    #[error("不支持的数据类型: {0}")]
    UnsupportedType(String),

    /// 压缩/解压错误
    #[error("压缩错误: {0}")]
    Compression(String),
}

/// DMS 操作结果类型别名
pub type Result<T> = std::result::Result<T, DmsError>;
