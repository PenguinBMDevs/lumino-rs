//! 播放状态子模块
//!
//! 由 Root 持有，存储播放相关状态。
//!
//! 注意：此模块从 `lumino-ui-core` 迁移而来（ui-core 是 UI 基础层，
//! 不应依赖 playback/midi-io 等业务 crate）。

use lumino_playback::{MidiTrackEvent, PlaybackManager, TempoChange};

/// 播放状态（由 Root 持有）
pub struct PlaybackState {
    /// 播放管理器
    pub manager: Option<PlaybackManager>,
    /// 延迟应用的 Tempo 变化（播放管理器未初始化时使用）
    pub pending_tempo_changes: Option<Vec<TempoChange>>,
    /// 延迟应用的 MIDI 输出连接（播放管理器未初始化时使用）
    pub pending_midi_output: Option<Box<dyn lumino_midi_io::OutputConnection>>,
    /// 每个音轨的 MIDI 控制事件（CC/PC/PB），播放時使用
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

impl Default for PlaybackState {
    fn default() -> Self {
        Self::new()
    }
}
