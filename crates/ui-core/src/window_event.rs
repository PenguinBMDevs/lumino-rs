//! Window 事件子模块
//!
//! 包括窗口事件枚举及其辅助类型。
//! 与 Window 结构体（管理窗口状态）分开存放，以免混入对 ui 模块的其他依赖。

use lumino_message::PerfData;

use crate::Message;

/// 窗口事件
#[derive(Debug, Clone)]
pub enum Event {
    Theme(String),
    Maximized(bool),
    Focused(bool),
    TrafficAction(TrafficAction),
    Drag,
    ToggleMaximize,
    Close,
    FpsUpdate(f32),
    PerfUpdate(PerfData),
}

/// 窗口控制（红绿灯）操作
#[derive(Debug, Clone, PartialEq)]
pub enum TrafficAction {
    Minimize,
    ToggleMaximize,
    Close,
}

impl Event {
    pub const fn theme(r: String) -> Message {
        Message::Window(Self::Theme(r))
    }
    pub const fn maximized(r: bool) -> Message {
        Message::Window(Self::Maximized(r))
    }
    pub const fn focused(r: bool) -> Message {
        Message::Window(Self::Focused(r))
    }
    pub fn traffic_action(action: &TrafficAction) -> Message {
        Message::Window(Self::TrafficAction(action.clone()))
    }
    pub const fn drag() -> Message {
        Message::Window(Self::Drag)
    }
    pub const fn toggle_maximize() -> Message {
        Message::Window(Self::ToggleMaximize)
    }
    pub const fn close() -> Message {
        Message::Window(Self::Close)
    }
    pub const fn fps_update(fps: f32) -> Message {
        Message::Window(Self::FpsUpdate(fps))
    }
    pub fn perf_update(data: PerfData) -> Message {
        Message::Window(Self::PerfUpdate(data))
    }
}
