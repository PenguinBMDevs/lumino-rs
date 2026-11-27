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

    /// 更新鼠标位置（tuple 与字段同形，避免 editor-state 依赖 ui-core）。
    pub fn update_cursor_position(&mut self, position: Option<(f32, f32)>) {
        if self.cursor_position == position {
            return;
        }
        self.cursor_position = position;
    }

    /// 更新 Canvas 偏移量。
    pub fn set_offset(&mut self, x: f32, y: f32) {
        self.offset_x = x;
        self.offset_y = y;
    }

    /// 更新 Canvas 尺寸。
    pub fn set_size(&mut self, width: f32, height: f32) {
        self.size_x = width;
        self.size_y = height;
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
