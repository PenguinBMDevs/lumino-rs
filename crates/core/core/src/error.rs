// Core 错误类型定义

use thiserror::Error;

/// Core 操作错误
#[derive(Error, Debug)]
pub enum CoreError {
    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// MIDI 解析错误
    #[error("MIDI 解析错误: {0}")]
    MidiParse(String),

    /// 缓存错误
    #[error("缓存错误: {0}")]
    Cache(String),

    /// 序列化错误
    #[error("序列化错误: {0}")]
    Serialization(String),

    /// 压缩/解压错误
    #[error("压缩错误: {0}")]
    Compression(String),

    /// 文件格式错误
    #[error("文件格式错误: {0}")]
    FileFormat(String),

    /// 无效参数
    #[error("无效参数: {0}")]
    InvalidArgument(String),

    /// 其他错误
    #[error("{0}")]
    Other(String),
}

/// Core 操作结果类型别名
pub type Result<T> = std::result::Result<T, CoreError>;

impl From<String> for CoreError {
    fn from(err: String) -> Self {
        CoreError::Other(err)
    }
}

impl From<&str> for CoreError {
    fn from(err: &str) -> Self {
        CoreError::Other(err.to_string())
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<bincode::Error> for CoreError {
    fn from(err: bincode::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<toml::de::Error> for CoreError {
    fn from(err: toml::de::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}

impl From<toml::ser::Error> for CoreError {
    fn from(err: toml::ser::Error) -> Self {
        CoreError::Serialization(err.to_string())
    }
}
