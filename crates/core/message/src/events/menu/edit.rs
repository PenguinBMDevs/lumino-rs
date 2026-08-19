#[derive(Debug, Clone)]
/// 编辑事件
pub enum Event {
    /// 撤销
    Undo,
    /// 重做
    Redo,
    /* */
    /// 剪切
    Cut,
    /// 复制
    Copy,
    /// 粘贴
    Paste,
    /// 全选
    SelectAll,
    /* */
    /// 查找
    Find,
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    /// 构造撤销事件
    pub const fn undo() -> Self {
        Self::Undo
    }
    /// 构造重做事件
    pub const fn redo() -> Self {
        Self::Redo
    }
    /// 构造剪切事件
    pub const fn cut() -> Self {
        Self::Cut
    }
    /// 构造复制事件
    pub const fn copy() -> Self {
        Self::Copy
    }
    /// 构造粘贴事件
    pub const fn paste() -> Self {
        Self::Paste
    }
    /// 构造全选事件
    pub const fn select_all() -> Self {
        Self::SelectAll
    }
    /// 构造查找事件
    pub const fn find() -> Self {
        Self::Find
    }
}
