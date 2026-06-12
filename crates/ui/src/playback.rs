//! 播放器模块 — 重新导出自 lumino-playback
//!
//! 保持与原有 `crate::playback::*` 路径完全兼容。

pub use lumino_playback::*;

// 重新导出子模块以保持深层路径兼容
pub use lumino_playback::core;
pub use lumino_playback::engine;
pub use lumino_playback::manager;
pub use lumino_playback::state;
pub use lumino_playback::tempo;
pub use lumino_playback::timeline;
