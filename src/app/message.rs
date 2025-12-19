use super::keyboard;
use super::router::Route;
use super::window;

#[derive(Debug, Clone)]
pub enum Message {
    RouteUpdated(Route),
    Window(window::Event),
    Keyboard(keyboard::Event),
    Null,
}
