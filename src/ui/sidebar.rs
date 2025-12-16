use iced::{
    Background, Border, Color, Element, widget::{
        Column, Container, Row, Space, button, container
    }
};

use crate::ui::{
    message::Message,
    router::{
        ROUTES,
        Route, RouteConfig,
    },
};

/* we'll allow the sidebar to expand for showing the tab hints. */
pub fn view<'a>(
    current: Route
) -> Element<'a, Message> {
    let inner = ROUTES.iter()
        .fold(
            Column::new().spacing(4),
            |col, cfg| {
                col.push(
                    item(cfg, current == cfg.route)

                )
            }
        );

    Container::new(inner)
        .width(46)
        .into()
}

/*
    button: 46x40
    split: 3x16
    icon: 16x16
    padding-y: 12
    padding-left: 3+9
    padding-right: 12
*/

fn item<'a>(
    cfg: &RouteConfig,
    active: bool,
) -> Element<'a, Message> {
    let icon = iced_font_awesome::fa_icon_solid(cfg.icon)
        .size(16.0)
        .color(Color::WHITE);

    let split = Container::new(
        Space::new()
    )
        .width(3)
        .height(16)
        .style(move |_| container::Style {
            background: active.then(||
                Background::Color(Color::WHITE)
            ),
            border: Border {
                radius: 1.5.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let inner = Row::new()
        .push(split)
        .push(icon)
        .spacing(12);

    button(inner)
        .width(46)
        .height(40)
        .padding([12, 0])
        .style(move |_, state| {
            use button::Status::*;
            let normal = Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.10)));
            let darker = Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)));
            let background = if active {
                match state {
                    Hovered => darker,
                    _ => normal
                }
            } else {
                match state {
                    Hovered => normal,
                    Pressed => darker,
                    _ => None,
                }
            };
            button::Style {
                background,
                border: Border {
                    radius: 6.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(Message::RouteUpdated(cfg.route))
        .into()
}
