use iced::{Background, Border, Color, Element, Length, widget::{Container, Text, container}};

use crate::app::Message;

pub fn view<'a>(
    version: &'static str,
) -> Element<'a, Message> {
    let inner = Text::new(
        format!("Hello Lumino v{version}"),
    )
        .width(Length::Fill)
        .height(Length::Fill)
        .size(32)
        .color(Color::WHITE)
        .center();
    Container::new(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(|_| container::Style {
            background: Some(Background::Color(
                Color::from_rgba(0.0, 0.0, 0.0, 0.15)
            )),
            border: Border {
                color: Color::from_rgb8(52, 54, 57),
                width: 1.0,
                radius: 8.into()
            },
            ..Default::default()
        })
        .into()
}
