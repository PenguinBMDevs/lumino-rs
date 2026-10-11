//! MIDI 数据模型错误类型

use thiserror::Error;

/// MIDI 模型错误
///
/// `Display` 走 `lumino_core::i18n`：文案目录按进程当前语言取词，
/// 因此这里的 `#[error(...)]` 用「取词 + 字段」两段式（thiserror 属性宏里
/// 没法直接调宏，但允许把任意表达式放进格式参数位置）。
#[derive(Error, Debug)]
pub enum LoaderError {
    /// IO 错误
    #[error("{}: {}", lumino_core::i18n::tr("IO 错误"), .0)]
    Io(#[from] std::io::Error),

    /// MIDI 解析错误
    #[error("{}: {}", lumino_core::i18n::tr("MIDI 解析错误"), .0)]
    MidiParse(String),

    /// 缓存错误
    #[error("{}: {}", lumino_core::i18n::tr("缓存错误"), .0)]
    Cache(String),

    /// 序列化错误
    #[error("{}: {}", lumino_core::i18n::tr("序列化错误"), .0)]
    Serialization(String),

    /// 压缩/解压错误
    #[error("{}: {}", lumino_core::i18n::tr("压缩错误"), .0)]
    Compression(String),

    /// 文件格式错误
    #[error("{}: {}", lumino_core::i18n::tr("文件格式错误"), .0)]
    FileFormat(String),

    /// 无效参数
    #[error("{}: {}", lumino_core::i18n::tr("无效参数"), .0)]
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
