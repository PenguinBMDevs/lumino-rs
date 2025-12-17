use iced::window;

use super::window::WindowEvent;
use super::router::Route;

#[derive(Debug, Clone)]
pub enum StateUpdated {
    WindowMaximized(bool),
    WindowId(Option<window::Id>)
}

#[derive(Debug, Clone)]
pub enum Message {
    RouteUpdated(Route),
    SyncState(StateUpdated),
    Window(WindowEvent),
}
