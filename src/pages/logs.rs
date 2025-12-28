use iced::{Element, widget::text};

use crate::{app::Message, pages::Page};

pub struct LogsPage {}

impl LogsPage {
    pub fn new() -> Self {
        /* TODO */
        Self {}
    }
}

impl Page for LogsPage {
    fn update(&mut self, message: Message) -> bool {
        match message {
            /* TODO */
            _ => false,
        }
    }

    fn view<'a>(&self) -> Element<'a, Message> {
        text("This is Logs").into()
    }
}
