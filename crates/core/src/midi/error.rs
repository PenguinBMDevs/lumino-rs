//! MIDI 文档加载错误类型

use thiserror::Error;

/// MIDI 文档加载错误
#[derive(Error, Debug)]
pub enum MidiError {
    #[error("MIDI 解析失败: {0}")]
    Parse(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

pub type MidiResult<T> = std::result::Result<T, MidiError>;
