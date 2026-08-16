//! Lumino Realtime Synthesizer
//!
//! 提供基于官方 `xsynth-realtime` crate（本地 vendor 分支，见 workspace `[patch.crates-io]`）
//! 的实时音频合成能力。将官方后端类型重新导出为 `lumino-realtime` 的公共 API。

#[cfg(feature = "xsynth-realtime")]
mod events;

#[cfg(feature = "xsynth-realtime")]
mod xsynth_realtime_backend;

#[cfg(feature = "xsynth-realtime")]
pub use events::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent, SynthEvent};

#[cfg(feature = "xsynth-realtime")]
pub use xsynth_core::channel_group::ThreadCount;

#[cfg(feature = "xsynth-realtime")]
pub use xsynth_realtime_backend::*;
