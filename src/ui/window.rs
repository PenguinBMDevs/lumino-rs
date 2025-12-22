use iced::{
    Alignment::Center,
    Element, Length, Theme,
    widget::{container, mouse_area, row, space},
};

use crate::{
    app::{
        Message,
        window::{Event::Traffic, Window, traffic::TrafficAction},
    },
    resources::icon,
};

mod menu;
mod traffic;

pub fn view<'a>(window: &'a Window) -> Element<'a, Message> {
    let inner = row![
        logo(),
        menu::view(),
        gap(),
        traffic::view(window.is_maximized, window.is_focused)
    ]
    .align_y(Center);
    let container = container(inner)
        .width(Length::Fill)
        .height(30)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(if window.is_focused {
                palette.background.neutral.color
            } else {
                palette.background.weaker.color
            })
        })
        .align_y(Center);
    mouse_area(container)
        .on_press(Message::Window(Traffic(TrafficAction::Drag)))
        .on_double_click(Message::Window(Traffic(TrafficAction::ToggleMaximize)))
        .into()
}

fn gap<'a>() -> Element<'a, Message> {
    container(space()).width(Length::Fill).height(30).into()
}

fn logo<'a>() -> Element<'a, Message> {
    // simply just a placeholer
    let icon = icon(icon::GitHub)
        // 16+2=20-2
        .width(18)
        .height(18);

    container(icon)
        // 8+46+8
        .width(62)
        .height(30)
        .center(62)
        .into()
}
