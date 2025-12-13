use iced::{
    Color, Element, Length, widget::{Container, Text}
};

use super::message::Message;

pub struct App {
    version: &'static str,
}

impl App {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn update(&mut self, _message: Message) {

    }

    fn content(&self) -> Element<'_, Message> {
        Text::new(
            format!("Hello Lumino v{}", self.version),
        )
            .width(Length::Fill)
            .height(Length::Fill)
            .size(32)
            .color(Color::WHITE)
            .center()
            .into()
    }

    pub fn view(&self) -> Element<'_, Message> {
        Container::new(self.content())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
