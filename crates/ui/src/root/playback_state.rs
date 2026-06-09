//! 播放状态管理
//!
//! 从 Root 中提取的播放相关状态，减少 Root 的字段数。

use crate::playback::{MidiTrackEvent, PlaybackManager, TempoChange};

/// 播放状态（从 Root 提取）
pub struct PlaybackState {
    /// 播放管理器
    pub manager: Option<PlaybackManager>,
    /// 延迟应用的 Tempo 变化（播放管理器未初始化时缓存）
    pub pending_tempo_changes: Option<Vec<TempoChange>>,
    /// 延迟应用的 MIDI 输出（播放管理器未初始化时缓存）
    pub pending_midi_output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
    /// 各音轨的 MIDI 控制事件（CC/PC/PB），供播放时使用
    pub track_midi_events: std::collections::HashMap<usize, Vec<MidiTrackEvent>>,
}

impl PlaybackState {
    pub fn new() -> Self {
        Self {
            manager: None,
            pending_tempo_changes: None,
            pending_midi_output: None,
            track_midi_events: std::collections::HashMap::new(),
        }
    }
}
