pub use crate::{sidebar::Event as Sidebar, window::Event as Window};

#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Sidebar(Sidebar),
    Progress(Option<(String, f64)>),
    Null,
}

pub const fn null() -> Message {
    Message::Null
}
