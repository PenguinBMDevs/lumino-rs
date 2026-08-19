//! Window 事件子模块
//!
//! 包括窗口事件枚举及其辅助类型。
//! 与 Window 结构体（管理窗口状态）分开存放，以免混入对 ui 模块的其他依赖。

use lumino_message::PerfData;

use crate::Message;

/// 窗口事件
#[derive(Debug, Clone)]
pub enum Event {
    /// 主题变更
    Theme(String),
    /// 最大化状态变更
    Maximized(bool),
    /// 焦点状态变更
    Focused(bool),
    /// 红绿灯（窗口控制）操作
    TrafficAction(TrafficAction),
    /// 开始窗口拖拽移动
    Drag,
    /// 切换最大化
    ToggleMaximize,
    /// 关闭窗口
    Close,
    /// 帧率更新
    FpsUpdate(f32),
    /// 性能数据更新
    PerfUpdate(PerfData),
}

/// 窗口控制（红绿灯）操作
#[derive(Debug, Clone, PartialEq)]
pub enum TrafficAction {
    /// 最小化窗口
    Minimize,
    /// 切换最大化
    ToggleMaximize,
    /// 关闭窗口
    Close,
}

impl Event {
    /// 构造"主题变更"的窗口消息
    pub const fn theme(theme: String) -> Message {
        Message::Window(Self::Theme(theme))
    }
    /// 构造"最大化状态变更"的窗口消息
    pub const fn maximized(maximized: bool) -> Message {
        Message::Window(Self::Maximized(maximized))
    }
    /// 构造"焦点状态变更"的窗口消息
    pub const fn focused(focused: bool) -> Message {
        Message::Window(Self::Focused(focused))
    }
    /// 构造"红绿灯操作"的窗口消息
    pub fn traffic_action(action: &TrafficAction) -> Message {
        Message::Window(Self::TrafficAction(action.clone()))
    }
    /// 构造"开始窗口拖拽移动"的窗口消息
    pub const fn drag() -> Message {
        Message::Window(Self::Drag)
    }
    /// 构造"切换最大化"的窗口消息
    pub const fn toggle_maximize() -> Message {
        Message::Window(Self::ToggleMaximize)
    }
    /// 构造"关闭窗口"的窗口消息
    pub const fn close() -> Message {
        Message::Window(Self::Close)
    }
    /// 构造"帧率更新"的窗口消息
    pub const fn fps_update(fps: f32) -> Message {
        Message::Window(Self::FpsUpdate(fps))
    }
    /// 构造"性能数据更新"的窗口消息
    pub fn perf_update(data: PerfData) -> Message {
        Message::Window(Self::PerfUpdate(data))
    }
}
