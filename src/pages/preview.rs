use iced::{Element, widget::Text};

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
        Text::new("This is Preview").into()
    }
}
