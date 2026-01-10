use iced_core::{Border, Color, Length};
use iced_widget::{button, container, row, svg};

use crate::{Element, Message, Theme, resources::icon, window};

use lumino_core::{Event, event};

#[derive(Debug, Clone)]
struct TrafficConfig {
    icon: TrafficIcon,
    color: Option<Color>,
    event: Event,
}

#[derive(Debug, Clone, Copy)]
enum TrafficIcon {
    Static(icon::Icon),
    Toggle {
        normal: icon::Icon,
        active: icon::Icon,
    },
}

const TRAFFICS: &[TrafficConfig] = &[
    TrafficConfig {
        icon: TrafficIcon::Static(icon::WindowMin),
        color: None,
        event: event!(Window.Minimize),
    },
    TrafficConfig {
        icon: TrafficIcon::Toggle {
            normal: icon::WindowMax,
            active: icon::WindowUnMax,
        },
        color: None,
        event: event!(Window.ToggleMaximize),
    },
    TrafficConfig {
        icon: TrafficIcon::Static(icon::WindowClose),
        color: Some(Color::from_rgb8(196, 43, 28)),
        event: event!(Window.Close),
    },
];

pub fn view<'a>(window: &'a window::Window) -> Element<'a> {
    let items = TRAFFICS
        .iter()
        .map(|cfg| item(cfg, window))
        .collect::<Vec<_>>();

    let inner = row(items).spacing(1);

    container(inner)
        /* 45+1+45+1+45 */
        .width(137)
        .height(Length::Fill)
        .into()
}

fn item<'a>(cfg: &'a TrafficConfig, window: &'a window::Window) -> Element<'a> {
    let icon = icon(match cfg.icon {
        TrafficIcon::Static(r) => r,
        TrafficIcon::Toggle { normal, active } => {
            if window.is_maximized {
                active
            } else {
                normal
            }
        }
    })
    .width(10)
    .height(10)
    .style(move |theme: &Theme, _| {
        let palette = theme.extended_palette();
        svg::Style {
            color: Some(if window.is_focused {
                palette.background.neutral.text
            } else {
                palette.background.strongest.color
            }),
        }
    });

    // 45px*29px matches the actual traffic buttons on Windows.
    let inner = container(icon).width(45).height(29).center(Length::Fill);

    button(inner)
        .on_press(Message::Core(cfg.event.clone()))
        .style(move |theme: &Theme, status| {
            use button::Status::*;

            let palette = theme.extended_palette();
            let background = match status {
                Hovered => cfg.color.unwrap_or(palette.background.weaker.color),

                Pressed => cfg
                    .color
                    // Make it darker.
                    .map(|c| Color::from_rgb(c.r * 0.9, c.g * 0.9, c.b * 0.9))
                    .unwrap_or(palette.background.weak.color),

                _ => Color::TRANSPARENT,
            };

            button::Style {
                // Remove the default rounding.
                border: Border::default().rounded(0),
                ..Default::default()
            }
            .with_background(background)
        })
        .into()
}
