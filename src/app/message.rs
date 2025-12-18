use iced::window;

use super::router::Route;
use super::window::WindowEvent;

#[derive(Debug, Clone)]
pub enum StateUpdated {
    WindowMaximized(bool),
    WindowId(Option<window::Id>),
}

#[derive(Debug, Clone)]
pub enum Message {
    RouteUpdated(Route),
    SyncState(StateUpdated),
    Window(WindowEvent),
    Null,
}
