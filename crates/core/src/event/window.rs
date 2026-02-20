#[derive(Debug, Clone)]
/// 窗口事件
pub enum Event {
    Drag,
    Close,
    ToggleMaximize,
    Maximize,
    Minimize,
}
