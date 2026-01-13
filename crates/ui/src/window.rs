use crate::{Message, Theme};

#[derive(Debug, Clone)]
pub enum Event {
    Theme(String),
    Maximized(bool),
    Focused(bool),
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
}

#[derive(Debug, Clone)]
pub struct Window {
    pub theme: Theme,
    pub is_maximized: bool,
    pub is_focused: bool,
}

fn get_theme(theme: &str) -> Theme {
    Theme::ALL
        .iter()
        .find(|t| t.to_string() == theme)
        .cloned()
        .unwrap_or(Window::default_theme())
}

impl Window {
    pub fn new(theme: &str) -> Self {
        Self {
            theme: get_theme(theme),
            is_maximized: false,
            is_focused: true,
        }
    }
    fn default_theme() -> Theme {
        Theme::TokyoNightStorm
    }
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Theme(r) => self.theme = get_theme(&r),
            Event::Maximized(r) => self.is_maximized = r,
            Event::Focused(r) => self.is_focused = r,
        }
    }
}
