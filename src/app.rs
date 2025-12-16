pub mod message;
pub mod window;

use iced::{
    Element, Task
};

pub use message::{
    Message,
    StateUpdated,
    TrafficAction,
};

use super::ui::{
    self,
    router::Route,
};

pub struct App {
    pub version: &'static str,
    pub route: Route,
    pub window: window::WindowState,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let this = Self {
            version: env!("CARGO_PKG_VERSION"),
            route: Route::Editor,
            window: Default::default(),
        };
        let task = window::latest()
            .map(|id| Message::SyncState(
                StateUpdated::WindowId(id)
            ));
        (this, task)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SyncState(state) => {
                use StateUpdated::*;
                match state {
                    WindowId(id) => self.window.id = id,
                    WindowMaximized(state) => self.window.is_maximized = state,
                }
            }
            Message::RouteUpdated(route) => {
                self.route = route;
            },
            Message::WindowTraffic(action) => {
                use TrafficAction::*;
                let Some(id) = self.window.id else {
                    return Task::none();
                };
                return match action {
                    WindowToggleMaximize => window::toggle_maximize(id),
                    WindowClose => window::close(id),
                    WindowMinimize => window::minimize(id, true),
                    WindowDrag => window::drag(id)
                }
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }
}
