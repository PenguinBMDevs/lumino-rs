//! Lumino Realtime Synthesizer
//!
//! 提供基于 xsynth 的实时音频合成能力，支持两种后端：
//!
//! - `realtime`（默认）：自定义渲染引擎，直接在 cpal 音频回调中驱动 `ChannelGroup` 渲染。
//! - `xsynth-realtime`：使用官方的 `xsynth-realtime` crate 作为后端。

#[cfg(feature = "realtime")]
mod config;

#[cfg(feature = "realtime")]
mod stats;

#[cfg(any(feature = "realtime", feature = "xsynth-realtime"))]
mod events;

#[cfg(feature = "realtime")]
pub mod engine;

#[cfg(feature = "realtime")]
mod synth;

#[cfg(feature = "xsynth-realtime")]
mod xsynth_realtime_backend;

#[cfg(feature = "realtime")]
pub use config::{SynthFormat, XSynthRealtimeConfig};

#[cfg(any(feature = "realtime", feature = "xsynth-realtime"))]
pub use events::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent, SynthEvent};

#[cfg(feature = "realtime")]
pub use stats::{RealtimeSynthStatsReader, RenderPerfStats};

#[cfg(feature = "realtime")]
pub use synth::RealtimeSynth;

#[cfg(any(feature = "realtime", feature = "xsynth-realtime"))]
pub use xsynth_core::channel_group::ThreadCount;

#[cfg(feature = "xsynth-realtime")]
pub use xsynth_realtime_backend::*;

/// 默认采样率
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;

/// 默认缓冲区大小（毫秒，仅用于配置兼容，直接渲染模式下不参与缓冲）
pub const DEFAULT_BUFFER_MS: f64 = 10.0;
