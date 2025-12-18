use iced::{Element, widget::text};

use crate::{app::Message, pages::Page};

pub struct PreviewPage {

}

impl PreviewPage {
    pub fn new() -> Self {
        /* TODO */
        Self {  }
    }
}

impl Page for PreviewPage {
    fn update(&mut self, message: Message) -> bool {
        match message {
            /* TODO */
            _ => false,
        }
    }

    fn view<'a>(&self) -> Element<'a, Message> {
        text("This is Preview").into()
    }
}
