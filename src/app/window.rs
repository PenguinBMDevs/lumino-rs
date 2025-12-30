pub mod menu;
pub mod traffic;

pub use iced::window::{Id, Settings, events, is_maximized, latest, settings};
use iced::{Task, Theme, window::Event as WindowEvent};

use super::Message;

use menu::MenuAction;
use traffic::TrafficAction;

#[derive(Debug, Clone)]
pub enum Event {
    Menu(MenuAction),
    Traffic(TrafficAction),
    System(WindowEvent),
    Update(Update),
}

#[derive(Debug, Clone)]
pub enum Update {
    Maximized(bool),
    Focused(bool),
    Id(Option<Id>),
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: Option<Id>,
    pub theme: Theme,
    pub title: String,
    pub is_maximized: bool,
    pub is_focused: bool,
}

impl Window {
    pub fn new() -> Self {
        Self {
            id: None,
            theme: Theme::CatppuccinMocha,
            title: "Lumino".into(),
            is_maximized: false,
            is_focused: true,
        }
    }
    pub fn traffic(&self, action: TrafficAction) -> Task<Message> {
        tracing::debug!(?self.id, ?action, "Window traffic action");
        let Some(id) = self.id else {
            return Task::none();
        };
        traffic::handle(id, action)
    }
    pub fn menu(&mut self, action: MenuAction) -> Task<Message> {
        tracing::debug!(?action, "Window menu action");
        menu::handle(action, self)
    }
    pub fn system(&mut self, event: WindowEvent) -> Task<Message> {
        tracing::debug!(?self.id, ?event, "Window system event");
        let Some(id) = self.id else {
            #[cfg(target_os = "macos")]
            crate::app::macos::menu::init();
            return Task::none();
        };
        use WindowEvent::*;
        match event {
            Resized(_) => {
                is_maximized(id).map(|r| Message::Window(Event::Update(Update::Maximized(r))))
            }
            Focused => Task::done(Message::Window(Event::Update(Update::Focused(true)))),
            Unfocused => Task::done(Message::Window(Event::Update(Update::Focused(false)))),
            _ => Task::none(),
        }
    }
    pub fn update(&mut self, event: Update) -> Task<Message> {
        use Update::*;
        tracing::info!(?event, "Window state updated");
        match event {
            Maximized(r) => self.is_maximized = r,
            Focused(r) => self.is_focused = r,
            Id(r) => self.id = r,
        }
        Task::none()
    }
}
