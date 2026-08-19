#[derive(Debug, Clone)]
/// 帮助事件
pub enum Event {
    /// 关于对话框
    About,
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    /// 构造关于对话框事件
    pub const fn about() -> Self {
        Self::About
    }
}
