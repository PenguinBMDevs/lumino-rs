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
    // ── 构造函数（替代 event! 宏） ──

    pub fn theme(t: String) -> Self {
        Self::Theme(t)
    }
    pub const fn zoom_in() -> Self {
        Self::ZoomIn
    }
    pub const fn zoom_out() -> Self {
        Self::ZoomOut
    }
    pub const fn zoom_reset() -> Self {
        Self::ZoomReset
    }
}
