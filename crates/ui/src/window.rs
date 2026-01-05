use crate::{
    Theme,
    message,
};

#[derive(Debug, Clone)]
pub struct Window {
    pub theme: Theme,
    pub is_maximized: bool,
    pub is_focused: bool,
}

impl Window {
    pub fn new() -> Self {
        Self {
            theme: Theme::CatppuccinMocha,
            is_maximized: false,
            is_focused: true,
        }
    }
    pub fn handle_event(&mut self, event: message::Window) {
        use message::Window::*;
        match event {
            Maximized(r) => self.is_maximized = r,
            Focused(r) => self.is_focused = r,
        }
    }
}
