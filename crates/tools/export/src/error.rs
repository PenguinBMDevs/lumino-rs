use std::path::PathBuf;
use thiserror::Error;

/// 导出错误类型
#[derive(Debug, Error)]
pub enum ExportError {
    /// IO 错误
    #[error("{}: {}", lumino_core::i18n::tr("IO 错误"), .0)]
    Io(#[from] std::io::Error),

    /// MIDI 写入错误
    #[error("{}: {}", lumino_core::i18n::tr("MIDI 写入错误"), .0)]
    MidiWrite(String),

    /// MIDI 解析错误
    #[error("{}: {}", lumino_core::i18n::tr("MIDI 解析错误"), .0)]
    MidiParse(String),

    /// 音频写入错误
    #[error("{}: {}", lumino_core::i18n::tr("音频写入错误"), .0)]
    AudioWrite(String),

    /// 无效的导出数据
    #[error("{}: {}", lumino_core::i18n::tr("无效的导出数据"), .0)]
    InvalidData(String),

    /// 文件路径错误
    #[error("{}: {}", lumino_core::i18n::tr("无效的文件路径"), .0.display())]
    InvalidPath(PathBuf),

    /// 编码错误
    #[error("{}: {}", lumino_core::i18n::tr("编码错误"), .0)]
    Encoding(String),

    /// 文件格式错误
    #[error("{}: {}", lumino_core::i18n::tr("文件格式错误"), .0)]
    FileFormat(String),

    /// 压缩/解压错误
    #[error("{}: {}", lumino_core::i18n::tr("压缩错误"), .0)]
    Compression(String),

    /// 用户主动中止
    ///
    /// ⚠️ 刻意不本地化：`Display` 输出被多处 `error == "已中止"` 直接字符串比较
    /// （见 `crates/editor/ui/src/host/dialog/state.rs`），一旦本地化比较即失效。
    #[error("已中止")]
    Aborted,
}

/// 导出结果类型
pub type ExportResult<T> = Result<T, ExportError>;

impl From<bincode::Error> for ExportError {
    fn from(err: bincode::Error) -> Self {
        ExportError::Encoding(err.to_string())
    }
}

impl From<serde_json::Error> for ExportError {
    fn from(err: serde_json::Error) -> Self {
        ExportError::Encoding(err.to_string())
    }
}
