pub use crate::{sidebar::Event as Sidebar, window::Event as Window};

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    Progress(Option<(String, f64)>),
    ScrollbarScrolled(f32), // 滚动条滚动事件，参数为新的scroll_x值
    ScrollbarScrolledY(f32), // 垂直滚动条滚动事件，参数为新的scroll_y值
    /// Canvas 位置和尺寸更新，用于坐标转换和边界检测
    CanvasBoundsChanged { offset: iced_core::Point, size: iced_core::Size },
    /// 菜单状态更新
    MenuStateChanged(bool), // true = 菜单打开，false = 菜单关闭
    Null,
}

pub const fn null() -> Message {
    Message::Null
}
