use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, row, space, svg};

use super::{Event, ROUTES, Route, RouteConfig};

use crate::{Element, Theme, resources::icon};

pub fn view<'a>(active: Route) -> Element<'a> {
    let items = ROUTES
        .into_iter()
        .map(|r| match r {
            RouteConfig::Item { route, icon } => item(route, icon, route == active),
            RouteConfig::Space => space().height(Length::Fill).into(),
        })
        .collect::<Vec<_>>();

    container(column(items))
        .width(48)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weaker.color)
        })
        .into()
}

fn item<'a>(route: Route, svg: icon::Icon, active: bool) -> Element<'a> {
    let split = container(space())
        .width(2)
        .height(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let background = match active {
                true => palette.primary.base.color,
                false => Color::TRANSPARENT,
            };

            container::Style::default().background(background)
        });

    let icon = icon(svg).width(20).style(move |theme: &Theme, _| {
        let palette = theme.extended_palette();
        let color = match active {
            true => palette.background.neutral.text,
            false => palette.background.strongest.color,
        };
        svg::Style { color: Some(color) }
    });

    let inner = row![split, icon,]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    button(inner)
        .width(48)
        .height(48)
        .padding(0)
        .style(move |theme: &Theme, status| {
            use button::Status::*;
            let palette = theme.extended_palette();
            let text_color = match status {
                Hovered | Pressed => palette.background.base.color,
                _ => palette.background.weakest.color,
            };
            button::Style {
                text_color,
                ..Default::default()
            }
            .with_background(Color::TRANSPARENT)
        })
        .on_press(Event::route_updated(route))
        .into()
}
