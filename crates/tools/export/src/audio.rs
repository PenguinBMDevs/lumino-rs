//! MIDI→音频渲染流水线
//!
//! 双模式：流式([`render_audio`]) 和 内存([`render_audio_from_document`])。
//!
//! 架构：AudioEngine → EventProcessor → SampleSink，参考 OmniConverter 的
//! MIDIConverter + EventsProcesser 设计。

pub mod codec;
pub mod config;
pub mod control;
pub mod engine;
pub mod event;
pub mod event_kind;
pub mod event_stream;
pub mod gpu_backend;
pub mod limiter;
pub mod render_loops;
pub mod renderer;
pub mod sink_factory;
pub mod stream;
pub mod tick_conv;

pub use config::AudioRenderConfig;
pub use engine::AudioEngine;
pub use render_loops::render_audio;
pub use render_loops::render_audio_from_document;
pub use stream::SampleSink;
