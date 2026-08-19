//! MIDI 播放模块
//!
//! 负责 MIDI 音符的播放，包括：
//! - 速度（Tempo）管理和 BPM 计算
//! - Tick 到实际时间的转换
//! - 音符事件调度
//! - 播放状态管理
//!
//! 原 `lumino-playback` crate 的内容已并入 `lumino-midi-io`。

pub mod engine;
pub mod manager;

pub use engine::{
    EventType, MidiMessage, MidiTrackEvent, NoteEvent, PlaybackEngine, ScheduledEvent,
};
pub use manager::PlaybackManager;

// 子模块
pub mod tempo;
pub mod timeline;

// 重新导出核心类型
pub use core::{Playback, PlaybackAccessor};
pub use state::PlaybackState;
pub use tempo::{TempoChange, bpm_from_tempo, tempo_from_bpm};
pub use timeline::Timeline;

// 核心模块（包含 Playback 结构体和 PlaybackAccessor trait）
pub mod core;
// 状态模块（包含 PlaybackState 枚举）
pub mod state;

#[cfg(test)]
pub mod tests;
