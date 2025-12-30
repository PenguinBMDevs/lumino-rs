pub mod keyboard;
pub mod macos;
pub mod message;
pub mod router;
pub mod window;
pub mod worker;

use iced::{Element, Subscription, Task, Theme};

pub use message::Message;

use super::{pages, ui};

use router::Route;

pub struct App {
    pub route: Route,
    pub window: window::Window,
    pub pages: pages::Pages,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let this = Self {
            route: Route::Editor,
            window: window::Window::new(),
            pages: pages::Pages::new(),
        };
        // Request and update the window Id.
        let task =
            window::latest().map(|r| Message::Window(window::Event::Update(window::Update::Id(r))));
        (this, task)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        tracing::trace!(?message, "Message");
        match message {
            Message::RouteUpdated(route) => {
                tracing::info!(from = ?self.route, to = ?route, "Route changed");
                self.route = route;
            }
            Message::Window(event) => {
                use window::Event::*;
                return match event {
                    Traffic(r) => self.window.traffic(r),
                    Menu(r) => self.window.menu(r),
                    System(r) => self.window.system(r),
                    Update(r) => self.window.update(r),
                };
            }
            Message::Keyboard(event) => {
                return keyboard::handle(event);
            }
            Message::Null => (),
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            // Window events, like resizing, focusing, etc.
            window::events().map(|(_, r)| Message::Window(window::Event::System(r))),
            // Keyboard events, like pressing, releasing, etc.
            keyboard::listen().map(Message::Keyboard),
            // Backend events, like menu messages on macOS.
            worker::subscription(),
        ])
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
