pub use crate::{sidebar::Event as Sidebar, window::Event as Window};

#[derive(Debug, Clone)]
pub enum EditorAction {
    Pressed(iced_core::Point),
    Moved(iced_core::Point),
    Released,
}

#[derive(Debug, Clone)]
pub enum AudioAction {
    PlayNote { key: u8, velocity: u8 },
    StopNote { key: u8 },
}

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    Progress(Option<(String, f64)>),
    ScrollbarScrolled(f32), // 滚动条滚动事件，参数为新的scroll_x值
    ScrollbarScrolledY(f32), // 垂直滚动条滚动事件，参数为新的scroll_y值
    ZoomXChanged { zoom: f32, fixed_ratio: f32 }, // 横向缩放事件，参数为新的zoom_x值和固定点比例(0.0=左边缘, 1.0=右边缘)
    ZoomYChanged { zoom: f32, fixed_ratio: f32 }, // 纵向缩放事件，参数为新的zoom_y值和固定点比例(0.0=上边缘, 1.0=下边缘)
    /// Canvas 位置和尺寸更新，用于坐标转换和边界检测
    CanvasBoundsChanged { offset: iced_core::Point, size: iced_core::Size },
    /// 菜单状态更新
    MenuStateChanged(bool), // true = 菜单打开，false = 菜单关闭
    EditorAction(EditorAction),
    AudioAction(AudioAction),
    Null,
}

pub const fn null() -> Message {
    Message::Null
}
