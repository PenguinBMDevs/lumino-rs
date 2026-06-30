//! 基于 xsynth-core 的实时音频引擎。
//!
//! 架构借鉴 yinhe-audio：
//! - 放弃 xsynth-realtime，直接使用 `xsynth-core::ChannelGroup`
//! - 自研无锁 SPSC ring buffer 解耦 cpal 回调与渲染线程
//! - renderer 线程同步渲染 + sample-accurate 事件派发
//! - seek 时完整 Chase 机制（重放 CC/PC/PB 状态）
//!
//! cpal 回调只做 `ring.pop_into()` + 静音填充，**永不阻塞**。

mod adapter;
mod audio_model;
mod audio_renderer;
mod audio_ring;
mod channel;
pub mod engine;
mod engine_render;
mod engine_state;
pub mod export;
mod prepare_model;
pub mod spawn;

pub use adapter::AudioCommandAdapter;
pub use spawn::{AudioCommand, AudioSpawnError, CpalAudioHandle, channels_for_model, spawn_cpal_audio};
