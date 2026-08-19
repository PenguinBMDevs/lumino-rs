//! 窗口生命周期事件

#[derive(Debug, Clone)]
/// 窗口生命周期事件
pub enum Event {
    /// 拖拽窗口
    Drag,
    /// 关闭窗口
    Close,
    /// 切换最大化状态
    ToggleMaximize,
    /// 最大化窗口
    Maximize,
    /// 最小化窗口
    Minimize,
}
