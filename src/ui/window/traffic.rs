use iced::{
    Border, Color, Element, Length, Theme,
    widget::{button, container, row, svg},
};

use crate::{
    app::{
        Message,
        window::{Event::Traffic, traffic::TrafficAction},
    },
    resources::icon,
};

#[derive(Debug, Clone)]
struct TrafficConfig {
    icon: TrafficIcon,
    color: Option<Color>,
    event: TrafficAction,
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
        event: TrafficAction::Minimize,
    },
    TrafficConfig {
        icon: TrafficIcon::Toggle {
            normal: icon::WindowMax,
            active: icon::WindowUnMax,
        },
        color: None,
        event: TrafficAction::ToggleMaximize,
    },
    TrafficConfig {
        icon: TrafficIcon::Static(icon::WindowClose),
        color: Some(Color::from_rgb8(196, 43, 28)),
        event: TrafficAction::Close,
    },
];

pub fn view<'a>(is_maxed: bool, is_focused: bool) -> Element<'a, Message> {
    let items = TRAFFICS
        .iter()
        .map(|cfg| traffic_item(cfg, is_maxed, is_focused))
        .collect::<Vec<_>>();

    let inner = row(items).spacing(1);

    container(inner)
        /* 45+1+45+1+45 */
        .width(137)
        .height(Length::Fill)
        .into()
}

fn traffic_item<'a>(cfg: &'a TrafficConfig, is_maxed: bool, is_focused: bool) -> Element<'a, Message> {
    let icon = icon(match cfg.icon {
        TrafficIcon::Static(r) => r,
        TrafficIcon::Toggle { normal, active } => {
            if is_maxed {
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
            color: Some(if is_focused {
                palette.background.neutral.text
            } else {
                palette.background.strongest.color
            }),
        }
    });

    // 45px*29px matches the actual traffic buttons on Windows.
    let inner = container(icon).width(45).height(29).center(Length::Fill);

    button(inner)
        .on_press(Message::Window(Traffic(cfg.event)))
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
