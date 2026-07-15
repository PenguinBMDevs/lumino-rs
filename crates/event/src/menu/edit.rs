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
