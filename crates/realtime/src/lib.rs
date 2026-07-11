//! Lumino Realtime Synthesizer
//!
//! 基于 xsynth-core 的实时音频合成器，提供高性能的 MIDI 渲染能力。
//! 直接在 cpal 音频回调中驱动 `ChannelGroup` 渲染，不经过 xsynth-realtime
//! 的 BufferedRenderer 包装层，消除锁竞争与线程间拷贝，显著降低延迟。

mod config;
pub mod engine;
mod events;
mod stats;
mod synth;

pub use config::{SynthFormat, XSynthRealtimeConfig};
pub use events::{ChannelAudioEvent, ChannelConfigEvent, ChannelEvent, ControlEvent, SynthEvent};
pub use stats::{RealtimeSynthStatsReader, RenderPerfStats};
pub use synth::RealtimeSynth;

// 重新导出 xsynth-core 的 ThreadCount
pub use xsynth_core::channel_group::ThreadCount;

/// 默认采样率
pub const DEFAULT_SAMPLE_RATE: u32 = 44100;

/// 默认缓冲区大小（毫秒，仅用于配置兼容，直接渲染模式下不参与缓冲）
pub const DEFAULT_BUFFER_MS: f64 = 10.0;
