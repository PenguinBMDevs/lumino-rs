pub use crate::{
    window::Event as Window,
    sidebar::Event as Sidebar,
};

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    Null
}

pub const fn null() -> Message {
    Message::Null
}
