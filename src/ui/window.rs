use iced::{Alignment::Center, Element, Length, widget::{Container, Row, Space, mouse_area}};

use crate::app::{
    Message,
    window::{self, WindowEvent, traffic::TrafficAction},
};

mod menu;
mod traffic;

pub fn view<'a>(
    window: &window::Window
) -> Element<'a, Message> {
    let inner = Row::new()
        .push(icon())
        .push(menu::view())
        .push(space())
        .push(traffic::view(window.is_maximized));
    let container = Container::new(inner)
        .width(Length::Fill)
        .height(30)
        .align_y(Center);
    mouse_area(container)
        .on_press(Message::Window(
            WindowEvent::Traffic(
                TrafficAction::WindowDrag
            )
        ))
        .into()
}

fn space<'a>() -> Element<'a, Message> {
    Container::new(Space::new())
        .width(Length::Fill)
        .height(30)
        .into()
}

fn icon<'a>() -> Element<'a, Message> {
    Container::new(Space::new())
        /* 8+46+8 */
        .width(62)
        .height(30)
        .into()
}
