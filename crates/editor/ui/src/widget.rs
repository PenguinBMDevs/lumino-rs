//! 自定义 Widget 辅助函数
//!
//! 提供悬浮提示（tooltip）等通用 UI 能力的封装。
//! 实现已下沉至 lumino-ui-core，此处仅 re-export 以保持既有调用路径。

pub use lumino_ui_core::widget::{with_tooltip, with_tooltip_bottom};
