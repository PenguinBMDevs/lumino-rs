//! MIDI 数据模型错误类型

use thiserror::Error;

/// MIDI 模型错误
#[derive(Error, Debug)]
pub enum LoaderError {
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

/// MIDI 模型结果类型别名
pub type LoaderResult<T> = std::result::Result<T, LoaderError>;

impl From<String> for LoaderError {
    fn from(err: String) -> Self {
        LoaderError::Other(err)
    }
}

impl From<&str> for LoaderError {
    fn from(err: &str) -> Self {
        LoaderError::Other(err.to_string())
    }
}

impl From<bincode::Error> for LoaderError {
    fn from(err: bincode::Error) -> Self {
        LoaderError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for LoaderError {
    fn from(err: serde_json::Error) -> Self {
        LoaderError::Serialization(err.to_string())
    }
}
