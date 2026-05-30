use std::path::PathBuf;
use thiserror::Error;

/// 导出错误类型
#[derive(Debug, Error)]
pub enum ExportError {
    /// IO 错误
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    /// DMS 写入错误
    #[error("DMS 写入错误: {0}")]
    DmsWrite(String),

    /// MIDI 写入错误
    #[error("MIDI 写入错误: {0}")]
    MidiWrite(String),

    /// MIDI 解析错误
    #[error("MIDI 解析错误: {0}")]
    MidiParse(String),

    /// 音频写入错误
    #[error("音频写入错误: {0}")]
    AudioWrite(String),

    /// 无效的导出数据
    #[error("无效的导出数据: {0}")]
    InvalidData(String),

    /// 文件路径错误
    #[error("无效的文件路径: {0}")]
    InvalidPath(PathBuf),

    /// 编码错误
    #[error("编码错误: {0}")]
    Encoding(String),
}

/// 导出结果类型
pub type ExportResult<T> = Result<T, ExportError>;
