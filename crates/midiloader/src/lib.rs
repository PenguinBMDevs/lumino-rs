//! # Lumino MIDI Loader
//!
//! 一个高性能的 MIDI 文件解析库，支持标准 MIDI 文件格式（SMF）。
//!
//! ## 特性
//!
//! - 使用内存映射文件实现高效读取
//! - 支持所有三种 MIDI 文件格式（0, 1, 2）
//! - 完整的 MIDI 事件解析
//! - 进度报告功能
//! - 零拷贝读取（尽可能）
//!
//! ## 快速开始
//!
//! ```rust,no_run
//! use lumino_midiloader::load;
//!
//! // 简单加载
//! let midi = load("song.mid").unwrap();
//! println!("Loaded {} tracks", midi.track_count());
//! ```
//!
//! ## 零拷贝模式（推荐用于大文件）
//!
//! ```rust,no_run
//! use lumino_midiloader::{MmapReader, MmapMidiLoader};
//!
//! let reader = MmapReader::open("song.mid").unwrap();
//! let loader = MmapMidiLoader::new();
//! let midi = loader.load(&reader).unwrap();
//! ```

pub mod error;
pub mod mmap_model;
pub mod mmap_parser;
pub mod model;
pub mod parser;
pub mod progress;
pub mod reader;

// 核心模型
pub use model::{
    CC, Division, Event, EventKind, Format, Header, MetaEvent, MidiFile, Note, SysExEvent, Track,
};

// 零拷贝模型
pub use mmap_model::{
    FastEvent, FastEventIter, FastEventKind, MmapEvent, MmapEventIter, MmapEventKind,
    MmapMidiFile, MmapTrack,
};

// 加载器
pub use parser::{LoadOptions, MidiLoader};
pub use mmap_parser::MmapMidiLoader;

// 读取器
pub use reader::{BinaryReader, ByteBuffer, MmapReader};

// 进度报告
pub use progress::{Progress, ProgressEvent, ProgressHandle, ProgressReporter};

// 错误类型
pub use error::{MidiloaderError, Result};

use std::path::Path;

/// 加载 MIDI 文件的便捷函数
///
/// # 参数
///
/// * `path` - MIDI 文件路径
///
/// # 返回
///
/// 成功时返回 `MidiFile`，失败时返回 `MidiloaderError`
///
/// # 示例
///
/// ```rust,no_run
/// use lumino_midiloader::load;
///
/// let midi = load("song.mid").unwrap();
/// println!("Loaded {} tracks", midi.track_count());
/// ```
pub fn load<P: AsRef<Path>>(path: P) -> Result<MidiFile> {
    MidiLoader::new().load(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_api() {
        // 测试基本 API 可用性
        let _options = LoadOptions::new();
        
        // 测试进度报告创建
        let (_handle, reporter) = ProgressHandle::new(1024);
        reporter.started(100);
        reporter.completed();
    }

    #[test]
    fn test_midi_file_structure() {
        // 测试 MidiFile 结构的基本功能
        let track = Track {
            name: Some("Test Track".to_string()),
            events: vec![
                Event {
                    delta_time: 0,
                    kind: EventKind::Meta(MetaEvent::TrackName("Test".to_string())),
                    channel: None,
                },
                Event {
                    delta_time: 480,
                    kind: EventKind::NoteOn(Note {
                        key: 60,
                        velocity: 100,
                    }),
                    channel: Some(0),
                },
            ],
        };

        let midi = MidiFile {
            header: Header {
                format: Format::SingleTrack,
                ntracks: 1,
                division: Division::TicksPerQuarter(480),
            },
            tracks: vec![track],
        };

        assert_eq!(midi.track_count(), 1);
        assert_eq!(midi.total_events(), 2);
        assert!(midi.find_track_by_name("Test Track").is_some());
    }

    #[test]
    fn test_byte_buffer() {
        let data = vec![0x00, 0x01, 0x02, 0x03];
        let mut reader = ByteBuffer::new(&data);
        
        assert_eq!(reader.len(), 4);
        assert_eq!(reader.read_u8(), Some(0x00));
        assert_eq!(reader.read_u16_be(), Some(0x0102));
    }
}
