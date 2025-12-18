pub mod message;
pub mod window;
pub mod router;

use iced::{
    Element, Subscription, Task, Theme
};

pub use message::{
    Message,
    StateUpdated,
};

use super::{
    ui,
    pages,
};

use router::Route;

pub struct App {
    // pub version: &'static str,
    pub route: Route,
    pub window: window::Window,
    pub pages: pages::Pages,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let this = Self {
            // version: env!("CARGO_PKG_VERSION"),
            route: Route::Editor,
            window: window::Window::new(),
            pages: pages::Pages::new(),
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
            Message::Window(event) => {
                use window::WindowEvent::*;
                return match event {
                    Traffic(r) => self.window.traffic(r),
                    Menu(r) => self.window.menu(r),
                }
            },
            Message::Null => (),
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        ui::view(self)
    }

    pub fn theme(&self) -> Theme {
        self.window.theme.clone()
    }

    pub fn title(&self) -> String {
        self.window.title.clone()
    }
}
