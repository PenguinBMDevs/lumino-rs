pub mod window;
pub mod sidebar;

use crate::{
    app::{
        App,
        Message,
        router::Route
    },
    pages::{
        self,
        Page
    }
};

use iced::{
    Background, Color, Element, Length, widget::{
        Column, Container, Row, container,
    }
};

pub fn view<'a>(
    app: &App
) -> Element<'a, Message> {
    let content = match app.route {
        Route::Editor => app.pages.editor.view(),
        Route::Preview => app.pages.preview.view(),
        /* TODO */
        _ => app.pages.editor.view(),
    };

    let inner = Row::new()
        .push(sidebar::view(&app.route))
        .push(pages::view(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .spacing(8);

    let main = Container::new(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(8)
        .style(|_| container::Style {
            background: Some(Background::Color(
                Color::from_rgba(0.0, 0.0, 0.0, 0.2)
            )),
            ..Default::default()
        });

    let inner = Column::new()
        .push(window::view(&app.window))
        .push(main);

    Container::new(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
