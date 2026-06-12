//! 事件系统 — 重新导出自 lumino-event
//!
//! 保持与原有 `crate::event::*` 路径完全兼容。

pub use lumino_event::*;

// 重新导出子模块以保持深层路径兼容（如 crate::event::menu::file::Event）
pub use lumino_event::menu;
pub use lumino_event::window;
