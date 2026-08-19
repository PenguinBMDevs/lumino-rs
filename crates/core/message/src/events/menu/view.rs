#[derive(Debug, Clone)]
/// 视图事件
pub enum Event {
    /// 主题切换（字符串为主题名）
    Theme(String),
    /// 放大（横向和纵向同时放大）
    ZoomIn,
    /// 缩小（横向和纵向同时缩小）
    ZoomOut,
    /// 重置缩放
    ZoomReset,
}

impl Event {
    // ── 构造函数（替代 event! 宏） ──

    /// 构造主题切换事件
    pub fn theme(t: String) -> Self {
        Self::Theme(t)
    }
    /// 构造放大事件
    pub const fn zoom_in() -> Self {
        Self::ZoomIn
    }
    /// 构造缩小事件
    pub const fn zoom_out() -> Self {
        Self::ZoomOut
    }
    /// 构造重置缩放事件
    pub const fn zoom_reset() -> Self {
        Self::ZoomReset
    }
}
