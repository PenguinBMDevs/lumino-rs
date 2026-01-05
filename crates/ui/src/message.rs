#[derive(Debug, Clone)]
pub enum Message {
    Core(lumino_core::Event),
    Window(Window),
    Null
}

pub const fn null() -> Message {
    Message::Null
}

/* Window start */
#[derive(Debug, Clone)]
pub enum Window {
    Maximized(bool),
    Focused(bool),
}
impl Window {
    pub const fn maximized(r: bool) -> Message {
        Message::Window(Self::Maximized(r))
    }
    pub const fn focused(r: bool) -> Message {
        Message::Window(Self::Focused(r))
    }
}
/* Window end */
