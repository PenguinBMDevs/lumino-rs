#[derive(Debug, Clone)]
/// 视图事件
pub enum Event {
    Theme(String),
    /// 放大（横向和纵向同时放大）
    ZoomIn,
    /// 缩小（横向和纵向同时缩小）
    ZoomOut,
    /// 重置缩放
    ZoomReset,
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::Theme(_) => "主题".to_string(),
            Self::ZoomIn => "放大".to_string(),
            Self::ZoomOut => "缩小".to_string(),
            Self::ZoomReset => "重置缩放".to_string(),
        }
    }
}
