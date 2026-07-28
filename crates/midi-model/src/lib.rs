//! lumino-midi-model — MIDI 数据模型层（纯数据类型 + 文档模型，无 I/O、无音频后端依赖）
//!
//! 该 crate 从 `lumino-midi-loader` 中剥离出核心数据模型类型，使 `lumino-core` 等上层 crate
//! 无需依赖加载器细节即可引用音域模型。
//!
//! 分层目标：
//! - `lumino-midi-model`（叶子）：`CompactEvent` / `EventKind` / `MidiDocument` / `NoteEvent` ...
//! - `lumino-midi-loader`（加载器）：重新导出所有模型类型，并实现加载/解析/流式/量化逻辑。
//! - `lumino-core`：仅依赖 `lumino-midi-model` 而非 `lumino-midi-loader`。

pub mod compact;
pub mod document;
pub mod error;
pub mod note_event;
pub mod note_info;
pub mod track;

pub use compact::{CompactEvent, EventKind};
pub use document::MidiDocument;
pub use error::{LoaderError, LoaderResult};
pub use note_event::NoteEvent;
pub use note_info::NoteInfo;
pub use track::{TrackManager, TrackView, TrackVisibility};
