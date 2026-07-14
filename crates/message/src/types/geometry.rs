//! 中立的 2D 几何类型（替代 `iced_core::Point` / `iced_core::Size`）
//!
//! 这些类型用于 `Message` / `EditorAction` 等跨模块消息。定义在 `lumino-message`
//! （domain 层）中，使 domain 层**不依赖 UI 框架**（`iced_core`）。
//!
//! UI 层在消息边界处按需与 `iced_core` 类型互转，保持调用链不变。

/// 中立的 2D 点
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

impl Point2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 中立的 2D 尺寸
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size2 {
    pub width: f32,
    pub height: f32,
}

impl Size2 {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}
