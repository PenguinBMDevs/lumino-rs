//! 实时合成器类型重新导出
//!
//! 原 `lumino-realtime` crate 的内容已并入 `lumino-midi-io`。

pub use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent};
pub use xsynth_core::channel_group::{SynthEvent, ThreadCount};
pub use xsynth_realtime::{
    RealtimeEventSender, RealtimeSynth, RealtimeSynthError, RealtimeSynthStatsReader,
    StreamRestartError, SynthFormat, XSynthRealtimeConfig,
};
