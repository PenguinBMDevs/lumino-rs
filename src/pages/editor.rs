use iced::{Element, widget::Text};

use crate::{app::Message, pages::Page};

pub struct EditorPage {

}

impl EditorPage {
    pub fn new() -> Self {
        /* TODO */
        Self {  }
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
        Text::new("This is Editor").into()
    }
}
