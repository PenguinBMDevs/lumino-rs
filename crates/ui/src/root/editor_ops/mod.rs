//! Root 编辑器操作子模块
//!
//! 该模块已按职责拆分为以下子模块：
//! - `audio`: 音频动作管理
//! - `track`: 音轨管理
//! - `midi`: MIDI 输出管理
//! - `dialog`: 对话框状态管理
//! - `collaboration`: 远程协作功能
//! - `playback`: 播放管理

pub mod audio;
pub mod collaboration;
pub mod dialog;
pub mod midi;
pub mod playback;
pub mod track;
