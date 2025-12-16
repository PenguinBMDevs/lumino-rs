
use iced::Task;
pub use iced::window::{
    Id, Settings, settings,
    latest, close, minimize, drag
};

use super::{
    Message,
    StateUpdated,
};

#[derive(Debug, Clone)]
pub struct WindowState {
    pub id: Option<Id>,
    pub is_maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            id: None,
            is_maximized: false,
        }
    }
}

pub fn toggle_maximize(id: Id) -> Task<Message> {
    iced::window::toggle_maximize(id)
        .chain(
            iced::window::is_maximized(id)
                .map(|state| Message::SyncState(
                    StateUpdated::WindowMaximized(state)
                ))
        )
}
