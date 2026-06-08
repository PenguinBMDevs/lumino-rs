#[derive(Debug, Clone)]
/// 帮助事件
pub enum Event {
    About,
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::About => "关于".to_string(),
        }
    }
}
