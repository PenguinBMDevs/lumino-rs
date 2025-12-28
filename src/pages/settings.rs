use iced::{Element, widget::text};

use crate::{app::Message, pages::Page};

pub struct SettingsPage {}

impl SettingsPage {
    pub fn new() -> Self {
        /* TODO */
        Self {}
    }
}

impl Page for SettingsPage {
    fn update(&mut self, message: Message) -> bool {
        match message {
            /* TODO */
            _ => false,
        }
    }

    fn view<'a>(&self) -> Element<'a, Message> {
        text("This is Settings").into()
    }
}
