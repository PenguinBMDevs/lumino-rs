//! 窗口生命周期事件

#[derive(Debug, Clone)]
pub enum Event {
    Drag,
    Close,
    ToggleMaximize,
    Maximize,
    Minimize,
}


