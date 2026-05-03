//! Canvas 状态管理

use iced_core::Point;

/// Canvas 状态（尺寸和偏移）
#[derive(Debug, Clone, Copy, Default)]
pub struct CanvasState {
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub offset: Point,
    /// Canvas 尺寸（宽, 高）
    pub size: Point,
    /// 当前鼠标在窗口中的位置
    pub cursor_position: Option<Point>,
}

impl CanvasState {
    pub fn new() -> Self {
        Self::default()
    }
}
