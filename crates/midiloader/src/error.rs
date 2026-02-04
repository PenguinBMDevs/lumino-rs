use thiserror::Error;

/// MIDI 加载器的结果类型
pub type Result<T> = std::result::Result<T, MidiloaderError>;

/// MIDI 加载器错误类型
#[derive(Error, Debug)]
pub enum MidiloaderError {
    /// IO 错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// 内存映射错误
    #[error("Memory map error: {0}")]
    Mmap(String),

    /// 无效的 MIDI 文件头
    #[error("Invalid MIDI header: {0}")]
    InvalidHeader(String),

    /// 无效的轨道数据
    #[error("Invalid track data: {0}")]
    InvalidTrackData(String),

    /// 无效的事件数据
    #[error("Invalid event data: {0}")]
    InvalidEventData(String),

    /// 不支持的格式
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    /// 文本事件中的无效 UTF-8
    #[error("Invalid UTF-8 in text event: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    /// 意外的文件结束
    #[error(
        "Unexpected end of file at position {position} (expected {expected} bytes, found {found})"
    )]
    UnexpectedEof {
        position: usize,
        expected: usize,
        found: usize,
    },

    /// 无效的变长数值
    #[error("Invalid variable-length quantity")]
    InvalidVarLen,

    /// 轨道索引超出范围
    #[error("Track index {index} out of range (max: {max})")]
    TrackIndexOutOfRange { index: usize, max: usize },

    /// 通道号无效
    #[error("Invalid channel number: {channel} (must be 0-15)")]
    InvalidChannel { channel: u8 },

    /// 音符号无效
    #[error("Invalid key number: {key} (must be 0-127)")]
    InvalidKey { key: u8 },
}

impl MidiloaderError {
    /// 创建无效的 MIDI 文件头错误
    pub fn invalid_header(msg: impl Into<String>) -> Self {
        Self::InvalidHeader(msg.into())
    }

    /// 创建无效的轨道数据错误
    pub fn invalid_track_data(msg: impl Into<String>) -> Self {
        Self::InvalidTrackData(msg.into())
    }

    /// 创建无效的事件数据错误
    pub fn invalid_event_data(msg: impl Into<String>) -> Self {
        Self::InvalidEventData(msg.into())
    }

    /// 创建不支持的格式错误
    pub fn unsupported_format(msg: impl Into<String>) -> Self {
        Self::UnsupportedFormat(msg.into())
    }

    /// 检查是否为 IO 错误
    pub fn is_io_error(&self) -> bool {
        matches!(self, Self::Io(_))
    }

    /// 检查是否为解析错误
    pub fn is_parse_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidHeader(_)
                | Self::InvalidTrackData(_)
                | Self::InvalidEventData(_)
                | Self::UnexpectedEof { .. }
                | Self::InvalidVarLen
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = MidiloaderError::invalid_header("test");
        assert!(matches!(err, MidiloaderError::InvalidHeader(_)));

        let err = MidiloaderError::invalid_track_data("test");
        assert!(matches!(err, MidiloaderError::InvalidTrackData(_)));

        let err = MidiloaderError::invalid_event_data("test");
        assert!(matches!(err, MidiloaderError::InvalidEventData(_)));

        let err = MidiloaderError::unsupported_format("test");
        assert!(matches!(err, MidiloaderError::UnsupportedFormat(_)));
    }

    #[test]
    fn test_error_checks() {
        let io_err = MidiloaderError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        assert!(io_err.is_io_error());
        assert!(!io_err.is_parse_error());

        let parse_err = MidiloaderError::UnexpectedEof {
            position: 100,
            expected: 4,
            found: 2,
        };
        assert!(!parse_err.is_io_error());
        assert!(parse_err.is_parse_error());
    }

    #[test]
    fn test_error_display() {
        let err = MidiloaderError::InvalidChannel { channel: 16 };
        let msg = format!("{}", err);
        assert!(msg.contains("16"));
        assert!(msg.contains("0-15"));
    }
}
