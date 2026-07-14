//! 窗口生命周期事件

#[derive(Debug, Clone)]
pub enum Event {
    Drag,
    Close,
    ToggleMaximize,
    Maximize,
    Minimize,
}

impl Event {
    pub fn display_name(&self) -> String {
        match self {
            Self::Drag => "拖动".to_string(),
            Self::Close => "关闭".to_string(),
            Self::ToggleMaximize => "切换最大化".to_string(),
            Self::Maximize => "最大化".to_string(),
            Self::Minimize => "最小化".to_string(),
        }
    }
}
