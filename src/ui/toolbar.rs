
use iced::{Background, Border, Color, Element, Length, widget::{Button, Container, Row, Space, Svg, Text, button, container, mouse_area, svg::{self, Handle}}};

use crate::app::{
    Message,
    TrafficAction,
};

const TOOLS: &[&'static str] = &[
    "Files",
    "Edit",
    "View",
    "Help"
];

#[derive(Debug, Clone)]
struct TrafficConfig {
    icon: TrafficIcon,
    color: Color,
    event: TrafficAction,
}

#[derive(Debug, Clone, Copy)]
enum TrafficIcon {
    Static(&'static [u8]),
    Toggle {
        normal: &'static [u8],
        active: &'static [u8],
    },
}

const TRAFFICS: &[TrafficConfig] = &[
    TrafficConfig {
        icon: TrafficIcon::Static(
            include_bytes!("../../resources/icons/min.svg")
        ),
        color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
        event: TrafficAction::WindowMinimize,
    },
    TrafficConfig {
        icon: TrafficIcon::Toggle {
            normal: include_bytes!("../../resources/icons/max.svg"),
            active: include_bytes!("../../resources/icons/unmax.svg"),
        },
        color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
        event: TrafficAction::WindowToggleMaximize
    },
    TrafficConfig {
        icon: TrafficIcon::Static(
            include_bytes!("../../resources/icons/close.svg")
        ),
        color: Color::from_rgb8(196, 43, 28),
        event: TrafficAction::WindowClose
    },
];

pub fn view<'a>(
    is_maxed: bool,
) -> Element<'a, Message> {

    let inner = Row::new()
        .push(icon())
        .push(tool())
        .push(traffic(is_maxed));

    Container::new(inner)
        .width(Length::Fill)
        .height(30)
        .into()
}

fn icon<'a>() -> Element<'a, Message> {
    Container::new(Space::new())
        .width(60)
        .height(30)
        .into()
}

fn tool<'a>() -> Element<'a, Message> {
    let inner = TOOLS.iter()
        .fold(
            Row::new().spacing(16),
            |row, cfg| {
                let tab = Text::new(*cfg).size(13);
                row.push(tab)
            }
        );

    let element = Container::new(inner)
        .width(Length::Fill)
        .height(30)
        .padding(8);

    mouse_area(element)
        .on_press(Message::WindowTraffic(
            TrafficAction::WindowDrag
        ))
        .into()
}

fn traffic<'a>(
    is_maxed: bool,
) -> Element<'a, Message> {
    let inner = TRAFFICS.iter()
        .fold(
            Row::new().spacing(1),
            |row, cfg| {
                row.push(traffic_item(cfg, is_maxed))
            }
        );
    Container::new(inner)
        .width(137)
        .height(Length::Fill)
        .into()

}

/*
TODO!: Automatically change color when received OnFocus event.
When the app is not focused, the entire toolbar should be darker.
*/
fn traffic_item<'a>(
    cfg: &'a TrafficConfig,
    is_maxed: bool,
) -> Element<'a, Message> {
    let icon_raw = match cfg.icon {
        TrafficIcon::Static(r) => r,
        TrafficIcon::Toggle { normal, active } => {
            if is_maxed {
                active
            } else {
                normal
            }
        }
    };

    let icon = Svg::new(Handle::from_memory(
        icon_raw
    ))
        .width(10)
        .height(10)
        .style(|_, _| svg::Style {
            color: Some(Color::WHITE)
        });

    let inner = Container::new(icon)
        .width(45)
        .height(29)
        .center(Length::Fill)
        .style(|_| container::Style {
            text_color: Some(cfg.color),
            ..Default::default()
        });

    Button::new(inner)
        .on_press(Message::WindowTraffic(
            cfg.event
        ))
        .style(move |_, state| {
            use button::Status::*;
            let background = match state {
                Pressed | Hovered => Some(Background::Color(cfg.color)),
                _ => None,
            };
            button::Style {
                background,
                border: Border {
                    radius: 0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}
