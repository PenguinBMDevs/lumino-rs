use iced::{Element, widget::text};

use crate::{app::Message, pages::Page};

pub struct EditorPage {}

impl EditorPage {
    pub fn new() -> Self {
        /* TODO */
        Self {}
    }
}

impl Page for EditorPage {
    fn update(&mut self, message: Message) -> bool {
        match message {
            /* TODO */
            _ => false,
        }
    }

    fn view<'a>(&self) -> Element<'a, Message> {
        text("This is Editor").into()
    }
}
