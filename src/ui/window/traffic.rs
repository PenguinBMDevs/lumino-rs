use iced::{Background, Border, Color, Element, Length, widget::{Button, Container, Row, Svg, button, container, svg::{self, Handle}}};

use crate::app::{Message, window::{WindowEvent, traffic::TrafficAction}};

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

macro_rules! include_res {
    ($path:literal) => {
        include_bytes!(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                $path
            )
        )
    };
}

const TRAFFICS: &[TrafficConfig] = &[
    TrafficConfig {
        icon: TrafficIcon::Static(
            include_res!("/resources/icons/min.svg")
        ),
        color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
        event: TrafficAction::WindowMinimize,
    },
    TrafficConfig {
        icon: TrafficIcon::Toggle {
            normal: include_res!("/resources/icons/max.svg"),
            active: include_res!("/resources/icons/unmax.svg"),
        },
        color: Color::from_rgba(1.0, 1.0, 1.0, 0.05),
        event: TrafficAction::WindowToggleMaximize
    },
    TrafficConfig {
        icon: TrafficIcon::Static(
            include_res!("/resources/icons/close.svg")
        ),
        color: Color::from_rgb8(196, 43, 28),
        event: TrafficAction::WindowClose
    },
];

pub fn view<'a>(
    is_maxed: bool
) -> Element<'a, Message> {
    let inner = TRAFFICS.iter()
        .fold(
            Row::new().spacing(1),
            |row, cfg| {
                row.push(traffic_item(cfg, is_maxed))
            }
        );
    Container::new(inner)
        /* 45+1+45+1+45 */
        .width(137)
        .height(Length::Fill)
        .into()
}

/*
TODO: Automatically change color when received OnFocus event.
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
        .on_press(Message::Window(
            WindowEvent::Traffic(
                cfg.event
            )
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
