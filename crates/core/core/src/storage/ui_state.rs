use serde::{Deserialize, Serialize};

/// 用户界面状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    /// 窗口左上角 X 坐标（None 表示未定位，由系统决定）
    pub x: Option<i32>,
    /// 窗口左上角 Y 坐标（None 表示未定位，由系统决定）
    pub y: Option<i32>,
    /// 窗口宽度（像素）
    pub w: u32,
    /// 窗口高度（像素）
    pub h: u32,
    /// 窗口是否处于最大化状态
    pub is_maximized: bool,
}

/// 用户界面状态默认值
impl Default for UiState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            w: 1440,             // 默认宽度1440像素
            h: 900,              // 默认高度900像素
            is_maximized: false, // 默认不是最大化状态
        }
    }
}
