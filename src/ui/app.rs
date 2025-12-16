use iced::{
    Element, Length, widget::{
        Column, Container, Row,
    }
};

use super::{
    message::Message,
    router::Route,
    toolbar,
    sidebar,
    content
};

pub struct App {
    version: &'static str,
    route: Route,
}

impl App {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            route: Route::Editor,
        }
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::RouteUpdated(route) => {
                self.route = route;
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let row = Row::new()
            .push(sidebar::view(self.route))
            .push(content::view(self.version))
            .spacing(8)
            .width(Length::Fill)
            .height(Length::Fill);

        let main = Container::new(row)
            .width(Length::Fill)
            .height(Length::Fill);

        let inner = Column::new()
            .push(toolbar::view())
            .push(main)
            .spacing(8);

        Container::new(inner)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(8)
            .into()
    }
}
