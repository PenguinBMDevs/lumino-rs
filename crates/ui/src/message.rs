pub use crate::{sidebar::Event as Sidebar, window::Event as Window};

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    ScrollbarScrolled(f32), // 滚动条滚动事件，参数为新的scroll_x值
    Null,
}

pub const fn null() -> Message {
    Message::Null
}
