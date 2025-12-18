pub mod menu;
pub mod traffic;

use iced::{Task, Theme};
pub use iced::window::{
    Id, Settings, settings, latest
};

use super::{
    Message,
    StateUpdated,
};

use menu::MenuAction;
use traffic::TrafficAction;

#[derive(Debug, Clone, Copy)]
pub enum WindowEvent {
    Menu(MenuAction),
    Traffic(TrafficAction)
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: Option<Id>,
    pub theme: Theme,
    pub title: String,
    /* TODO: Consider using atomics. */
    pub is_maximized: bool,
}

impl Window {
    pub fn new() -> Self {
        Self {
            id: None,
            theme: Theme::CatppuccinMocha,
            title: "Lumino".into(),
            is_maximized: false,
        }
    }
    pub fn traffic(&self, action: TrafficAction) -> Task<Message> {
        let Some(id) = self.id else {
            return Task::none();
        };
        traffic::handle(id, action)
    }
    pub fn menu(&mut self, action: MenuAction) -> Task<Message> {
        let Some(id) = self.id else {
            return Task::none();
        };
        menu::handle(id, action, self)
    }
}
