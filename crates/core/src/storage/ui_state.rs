use serde::{Deserialize, Serialize};

/// 用户界面状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub w: u32,
    pub h: u32,
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
