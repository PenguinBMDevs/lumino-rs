//! 实时合成器类型重新导出
//!
//! 原 `lumino-realtime` crate 的内容已并入 `lumino-midi-io`。

use std::sync::{Arc, Mutex};

pub use xsynth_core::channel::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent};
pub use xsynth_core::channel_group::{SynthEvent, ThreadCount};
pub use xsynth_realtime::{
    ChannelMix, RealtimeEventSender, RealtimeSynth, RealtimeSynthError, RealtimeSynthStatsReader,
    StreamRestartError, SynthFormat, XSynthRealtimeConfig,
};

/// 混音参数共享句柄类型。
///
/// 外层 `Arc<Mutex<…>>` 指针稳定（已创建的输出连接持有其克隆），
/// 重建合成管线时仅替换内层 `Arc<Vec<ChannelMix>>`，连接自动跟随新管线 ——
/// 与 `sender_shared` 同生命周期语义。
pub(crate) type ChannelMixHandle = Arc<Mutex<Arc<Vec<ChannelMix>>>>;
