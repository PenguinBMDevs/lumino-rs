//! 滚动条 Widget 模块
//!
//! 提供自定义滚动条组件，支持水平和垂直方向，支持拖拽滚动和边缘缩放。
//!
//! 子模块：
//! - `types`: 滚动条类型定义（方向、边缘、状态）
//! - `widget`: ScrollbarWidget 结构体和辅助方法
//! - `widget_impl`: iced Widget trait 实现

// 子模块
mod types;
mod widget;
mod widget_impl;

// 公开导出
pub use types::{Edge, ScrollbarOrientation, ScrollbarState};
pub use widget::ScrollbarWidget;
