//! MIDI 录制端到端集成测试
//!
//! 按场景拆分为三个子模块：
//! - `basic` — 基础录制测试（含 Mock MIDI 基础设施）
//! - `playback` — 播放相关测试
//! - `advanced` — 高级功能测试
//!
//! 运行方式：
//!   cargo test --test midi_recording

// TODO: 这些子模块尚未实现，先注释掉以避免编译失败
// mod advanced;
// pub(crate) mod basic;
// mod playback;
