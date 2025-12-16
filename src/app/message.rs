use iced::window;
use crate::ui::router::Route;

#[derive(Debug, Clone)]
pub enum StateUpdated {
    WindowMaximized(bool),
    WindowId(Option<window::Id>)
}

#[derive(Debug, Clone, Copy)]
pub enum TrafficAction {
    WindowMinimize,
    WindowToggleMaximize,
    WindowClose,
    WindowDrag,
}

#[derive(Debug, Clone)]
pub enum Message {
    RouteUpdated(Route),
    SyncState(StateUpdated),
    WindowTraffic(TrafficAction),
}
