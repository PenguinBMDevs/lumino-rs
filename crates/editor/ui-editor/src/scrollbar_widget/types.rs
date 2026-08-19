/// 滚动条方向。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarOrientation {
    /// 水平滚动条
    Horizontal,
    /// 垂直滚动条
    Vertical,
}

/// 滚动条滑块边缘（用于缩放拖拽）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    /// 起始边缘
    Start,
    /// 结束边缘
    End,
}

/// 滚动条交互状态。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ScrollbarState {
    /// 空闲状态
    #[default]
    Idle,
    /// 悬停在滑块上
    Hover,
    /// 悬停在滑块边缘
    HoverEdge(Edge),
    /// 拖拽滑块进行滚动
    Dragging {
        /// 拖拽起始的鼠标位置
        start_pos: f32,
        /// 拖拽起始的滚动值
        start_scroll: f32,
    },
    /// 拖拽滑块边缘进行缩放
    DraggingEdge {
        /// 拖拽起始的鼠标位置
        start_pos: f32,
        /// 拖拽起始的缩放值
        start_zoom: f32,
        /// 拖拽起始的滑块尺寸
        start_thumb_size: f32,
        /// 被拖拽的边缘
        edge: Edge,
    },
}
