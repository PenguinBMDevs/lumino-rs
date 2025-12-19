use iced::{Task, window};

use super::Message;

#[derive(Debug, Clone, Copy)]
pub enum TrafficAction {
    Minimize,
    ToggleMaximize,
    Close,
    Drag,
}

pub fn handle(id: window::Id, event: TrafficAction) -> Task<Message> {
    use TrafficAction::*;
    match event {
        Minimize => window::minimize(id, true),
        ToggleMaximize => window::toggle_maximize(id),
        Close => window::close(id),
        Drag => window::drag(id),
    }
}
