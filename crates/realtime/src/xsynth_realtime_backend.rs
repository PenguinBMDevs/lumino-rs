//! xsynth-realtime 后端封装
//!
//! 将官方的 `xsynth-realtime` crate 的类型重新导出为 `lumino-realtime` 的公共 API。
//! 当启用 `xsynth-realtime` feature 时，此模块替代 `realtime` feature 的自定义引擎。
//!
//! 注意：`SynthEvent`、`ThreadCount` 等类型已由 `events` 模块和 `xsynth_core` 统一导出，
//! 此处不再重复导出以避免 glob 冲突。

#[allow(unused_imports)]
pub use xsynth_realtime::{
    RealtimeEventSender, RealtimeSynth, RealtimeSynthError, RealtimeSynthStatsReader, SynthFormat,
    XSynthRealtimeConfig,
};
