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
