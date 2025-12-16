pub mod router;
pub mod toolbar;
pub mod sidebar;
pub mod content;

use crate::app::{
    App,
    Message
};

use iced::{
    Element, Length, widget::{
        Column, Container, Row,
    }
};

pub fn view<'a>(
    app: &App
) -> Element<'a, Message> {
    let row = Row::new()
        .push(sidebar::view(app.route))
        .push(content::view(app.version))
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill);

    let main = Container::new(row)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8);

    let inner = Column::new()
        .push(toolbar::view(app.window.is_maximized))
        .push(main);

    Container::new(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
