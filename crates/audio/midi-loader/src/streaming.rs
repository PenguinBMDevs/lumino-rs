//! 流式 MIDI 解析器——零事件常驻，逐事件按 tick 互锁多轨输出。
//!
//! 与 `MidiDocument`（全量加载、随机访问）不同，本模块为一次性顺序消费设计：
//!
//! - 基于 `midly::mmap` 零拷贝，不分配任何事件数据
//! - 每轨仅保持一个 `MmapEventIter` + 一个预读 peek 事件
//! - 多轨自动按 tick 互锁交织，正确处理 MIDI Format 1/2
//! - 适用于音频导出、流式传输等只需顺序访问的场景
//!
//! # 用法
//!
//! ```rust,ignore
//! use lumino_midi_loader::StreamingMidiPlayer;
//!
//! let bytes = std::fs::read("song.mid")?;
//! let mut player = StreamingMidiPlayer::from_bytes(&bytes)?;
//!
//! while let Some((tick, track_idx, kind)) = player.next_event() {
//!     println!("tick={}, track={}, event={:?}", tick, track_idx, kind);
//! }
//! ```
//!
//! 实现按职责拆分（保持各文件 < 400 行）：
//! - `cursor`（streaming_cursor.rs）：每轨前进游标
//! - `player`（streaming_player.rs）：播放器本体与 tempo 预扫描

#[path = "streaming_cursor.rs"]
mod cursor;

#[path = "streaming_player.rs"]
mod player;

pub use player::StreamingMidiPlayer;

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
