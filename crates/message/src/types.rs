//! lumino-message 内部共享类型和 Message 枚举定义
//!
//! 按领域拆分为以下子模块：
//!
//! | 模块 | 领域 | 包含类型 |
//! |------|------|----------|
//! | `message` | 消息中枢 | Message 枚举 + null() 辅助函数 |
//! | `editor` | 编辑器 | EditorAction |
//! | `audio` | 音频/导出 | AudioChannels, AudioFormat, ThreadingOption, Interpolation |
//! | `collab` | 协作 | (预留) |
//! | `midi` | MIDI | CcOption, CC_CONTROLLER_NAMES |
//! | `ui` | UI | PerfData, TupletType, SpeedFactor |

pub mod audio;
pub mod editor;
pub mod geometry;
pub mod message;
pub mod midi;
pub mod ui;

pub use audio::*;
pub use editor::*;
pub use geometry::*;
pub use message::*;
pub use midi::*;
pub use ui::*;
