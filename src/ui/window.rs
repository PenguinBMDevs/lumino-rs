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

pub use menu::{
    menus,
    MenuItem,
};

mod menu;
mod traffic;

pub fn view<'a>(window: &'a Window) -> Element<'a, Message> {
    let container = container(inner(window))
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

fn inner<'a>(window: &'a Window) -> Element<'a, Message> {
    // Use cfg! instead of #[cfg] to avoid annoying unused warning on macOS.
    // Unreachable branch will be optimized out anyway.
    if cfg!(target_os = "macos") {
        space().into()
    } else {
        row![
            logo(),
            menu::view(),
            gap(),
            traffic::view(window.is_maximized, window.is_focused)
        ]
        .align_y(Center).into()
    }
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
