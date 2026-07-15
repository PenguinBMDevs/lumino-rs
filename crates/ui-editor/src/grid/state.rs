//! Canvas 状态管理

use iced_core::Point;

/// 钢琴卷帘网格交互状态
#[derive(Debug)]
pub struct GridInteractionState {
    /// 鼠标位置
    pub position: Option<Point>,
    /// 上次点击时间（用于双击检测）
    pub last_click_time: std::time::Instant,
    /// 上次点击位置
    pub last_click_pos: Option<Point>,
    /// Shift 键是否按下
    pub shift_pressed: bool,
    /// 循环区域是否正在拖拽（同步标记，无需等消息处理）
    pub is_loop_dragging: bool,
    /// 演奏指示线是否正在被拖拽
    pub is_dragging_indicator: bool,
}

impl Default for GridInteractionState {
    fn default() -> Self {
        Self {
            position: None,
            last_click_time: std::time::Instant::now(),
            last_click_pos: None,
            shift_pressed: false,
            is_loop_dragging: false,
            is_dragging_indicator: false,
        }
    }
}
