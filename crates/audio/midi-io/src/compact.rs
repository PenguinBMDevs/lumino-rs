//! 紧凑型 MIDI 事件格式
//!
//! 定义已迁移至 `lumino-midi-model` crate（纯数据层，无音频后端依赖）。
//! 此处保留 `pub use` 以维持 `lumino_midi_io::compact` 的向后兼容，避免调用链断裂。
//!
//! 新代码请直接使用 `lumino_midi_model::compact::{CompactEvent, EventKind}`。

pub use lumino_midi_model::compact::{CompactEvent, EventKind};
