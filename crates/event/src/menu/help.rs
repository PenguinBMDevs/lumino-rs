#[derive(Debug, Clone)]
/// 帮助事件
pub enum Event {
    About,
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    pub const fn about() -> Self {
        Self::About
    }
}
