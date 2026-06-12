//! 全局常量定义 — 重新导出自 lumino-constants
//!
//! 保持与原有 `crate::constants::*` 路径完全兼容。

pub use lumino_constants::*;

// 重新导出子模块以保持深层路径兼容（如 crate::constants::editor::DEFAULT_MIN_TICKS）
pub use lumino_constants::dimensions;
pub use lumino_constants::editor;
pub use lumino_constants::rendering;
pub use lumino_constants::memory;
pub use lumino_constants::progress;
pub use lumino_constants::spacing;
pub use lumino_constants::scrollbar;
pub use lumino_constants::onion_skin;
