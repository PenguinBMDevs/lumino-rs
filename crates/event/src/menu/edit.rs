#[derive(Debug, Clone)]
/// 编辑事件
pub enum Event {
    Undo,
    Redo,
    /* */
    Cut,
    Copy,
    Paste,
    SelectAll,
    /* */
    Find,
}

impl Event {
    /// 获取事件的人类可读显示名称
    pub fn display_name(&self) -> String {
        match self {
            Self::Undo => "撤销".to_string(),
            Self::Redo => "重做".to_string(),
            Self::Cut => "剪切".to_string(),
            Self::Copy => "复制".to_string(),
            Self::Paste => "粘贴".to_string(),
            Self::SelectAll => "全选".to_string(),
            Self::Find => "查找".to_string(),
        }
    }

    // ── 构造函数（替代 event! 宏） ──

    pub const fn undo() -> Self {
        Self::Undo
    }
    pub const fn redo() -> Self {
        Self::Redo
    }
    pub const fn cut() -> Self {
        Self::Cut
    }
    pub const fn copy() -> Self {
        Self::Copy
    }
    pub const fn paste() -> Self {
        Self::Paste
    }
    pub const fn select_all() -> Self {
        Self::SelectAll
    }
    pub const fn find() -> Self {
        Self::Find
    }
}
