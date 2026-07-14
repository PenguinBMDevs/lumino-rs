//! Canvas 状态与视图几何

/// Canvas 状态（尺寸和偏移）
#[derive(Debug, Clone, Copy, Default)]
pub struct CanvasState {
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub offset_x: f32,
    /// Canvas 在窗口中的偏移量（用于坐标转换）
    pub offset_y: f32,
    /// Canvas 尺寸（宽, 高）
    pub size_x: f32,
    /// Canvas 尺寸（宽, 高）
    pub size_y: f32,
    /// 当前鼠标在窗口中的位置
    pub cursor_position: Option<(f32, f32)>,
}

impl CanvasState {
    /// 创建默认 Canvas 状态
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_state_default() {
        let cs = CanvasState::default();
        assert_eq!(cs.offset_x, 0.0);
        assert_eq!(cs.offset_y, 0.0);
        assert_eq!(cs.size_x, 0.0);
        assert_eq!(cs.size_y, 0.0);
        assert!(cs.cursor_position.is_none());
    }

    #[test]
    fn test_canvas_state_new() {
        let cs = CanvasState::new();
        assert_eq!(cs.offset_x, 0.0);
        assert!(cs.cursor_position.is_none());
    }
}
