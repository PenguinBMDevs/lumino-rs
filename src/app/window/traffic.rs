use iced::{Task, window};

use super::{Message, StateUpdated};

#[derive(Debug, Clone, Copy)]
pub enum TrafficAction {
    WindowMinimize,
    WindowToggleMaximize,
    WindowClose,
    WindowDrag,
}

pub fn handle(id: window::Id, event: TrafficAction) -> Task<Message> {
    use TrafficAction::*;
    match event {
        WindowMinimize => window::minimize(id, true),
        WindowToggleMaximize => window::toggle_maximize(id),
        WindowClose => window::close(id),
        WindowDrag => window::drag(id),
    }
    .chain(
        window::is_maximized(id)
            .map(|state| Message::SyncState(StateUpdated::WindowMaximized(state))),
    )
}
