//! lumino-message 内部共享类型
//!
//! 这些类型原本分散在 lumino-ui 的各个模块中（state::root_state, toolbar::types,
//! editor::velocity, statusbar::performance），因为被 Message 枚举引用而被提取到此处。
//! 跨 crate 共享的领域类型（AudioAction, DotType, NotePrecision, Tool）位于
//! `lumino-core`，请通过 `lumino_message::*` 的 re-export 使用。
//!
//! 按领域拆分为以下子模块：
//!
//! | 模块 | 领域 | 包含类型 |
//! |------|------|----------|
//! | `editor` | 编辑器 | EditorAction |
//! | `audio` | 音频/导出 | AudioChannels, AudioFormat, ThreadingOption, Interpolation |
//! | `collab` | 协作 | (预留) |
//! | `midi` | MIDI | CC_CONTROLLER_NAMES |
//! | `ui` | UI | PerfData, TupletType, SpeedFactor |

pub mod audio;
pub mod editor;
pub mod geometry;
pub mod midi;
pub mod ui;

pub use audio::*;
pub use editor::*;
pub use geometry::*;
pub use midi::*;
pub use ui::*;
