//! 播放器模块
//!
//! 负责MIDI音符的播放，包括：
//! - 速度（Tempo）管理和BPM计算
//! - Tick到实际时间的转换
//! - 音符事件调度
//! - 播放状态管理

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

// 核心模块（包含Playback结构体和PlaybackAccessor trait）
pub mod core;
// 状态模块（包含PlaybackState枚举）
pub mod state;

#[cfg(test)]
mod tests;
